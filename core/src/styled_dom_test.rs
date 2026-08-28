#[cfg(test)]
mod audit_tests {
    use super::*;
    use azul_css::props::basic::StyleFontFamily;

    fn fam(name: &str) -> StyleFontFamily {
        StyleFontFamily::System(name.to_string().into())
    }

    #[test]
    fn style_font_families_hash_is_length_sensitive() {
        // The length prefix guarantees that lists of different lengths cannot
        // collide, and that hashing is deterministic.
        let a = StyleFontFamiliesHash::new(&[fam("Arial")]);
        let a2 = StyleFontFamiliesHash::new(&[fam("Arial")]);
        assert_eq!(a, a2, "hash must be deterministic");

        let two = StyleFontFamiliesHash::new(&[fam("Arial"), fam("Helvetica")]);
        assert_ne!(a, two, "different-length family lists must not collide");

        let empty = StyleFontFamiliesHash::new(&[]);
        assert_ne!(empty, a);
        assert_ne!(empty, two);

        // Order still matters.
        let rev = StyleFontFamiliesHash::new(&[fam("Helvetica"), fam("Arial")]);
        assert_ne!(two, rev);
    }
}

#[cfg(test)]
#[allow(clippy::too_many_lines)]
mod autotest_generated {
    use azul_css::{dynamic_selector::PseudoStateFlags, props::basic::StyleFontFamily};

    use super::*;

    // ---------------------------------------------------------------------
    // helpers
    // ---------------------------------------------------------------------

    /// Builds a `NodeHierarchyItem` directly from the RAW (1-based) encoding:
    /// `0` = none, `n` = `NodeId(n - 1)`.
    const fn raw_item(parent: usize, prev: usize, next: usize, last: usize) -> NodeHierarchyItem {
        NodeHierarchyItem {
            parent,
            previous_sibling: prev,
            next_sibling: next,
            last_child: last,
        }
    }

    /// `<body>` with `n` leaf `<div>` children, cascaded against an empty stylesheet.
    /// Node ids are `0 = body`, `1..=n` = the children.
    fn flat_body(n: usize) -> StyledDom {
        let children: Vec<Dom> = (0..n).map(|_| Dom::create_div()).collect();
        let mut dom = Dom::create_body().with_children(children.into());
        StyledDom::create(&mut dom, Css::empty())
    }

    /// `<body> > <div> > <div>` — the last direct child of the root is itself a parent.
    fn nested_body() -> StyledDom {
        let mut dom = Dom::create_body().with_children(
            vec![Dom::create_div().with_children(vec![Dom::create_div()].into())].into(),
        );
        StyledDom::create(&mut dom, Css::empty())
    }

    fn parse_css(s: &str) -> Css {
        azul_css::parser2::new_from_str(s).0
    }

    fn family(name: &str) -> StyleFontFamily {
        StyleFontFamily::System(name.to_string().into())
    }

    const fn pseudo_flags(all: bool) -> PseudoStateFlags {
        PseudoStateFlags {
            hover: all,
            active: all,
            focused: all,
            disabled: all,
            checked: all,
            focus_within: all,
            visited: all,
            backdrop: all,
            dragging: all,
            drag_over: all,
        }
    }

    fn empty_menu() -> Menu {
        let items: Vec<crate::menu::MenuItem> = Vec::new();
        Menu::create(items.into())
    }

    // ---------------------------------------------------------------------
    // RestyleResult (predicate + merge)
    // ---------------------------------------------------------------------

    #[test]
    fn restyle_result_default_reports_no_changes() {
        let r = RestyleResult::default();
        assert!(!r.has_changes());
        assert!(!r.needs_layout);
        assert!(!r.needs_display_list);
        assert!(!r.gpu_only_changes);
        assert_eq!(r.max_relayout_scope, RelayoutScope::None);
    }

    #[test]
    fn restyle_result_has_changes_keys_off_node_map_not_property_count() {
        // A node entry with an EMPTY change list still counts as "changed":
        // has_changes() only looks at the node map, never at the inner Vec.
        let mut r = RestyleResult::default();
        r.changed_nodes.insert(NodeId::ZERO, Vec::new());
        assert!(r.has_changes());

        r.changed_nodes.clear();
        assert!(!r.has_changes());
    }

    #[test]
    fn restyle_result_merge_ors_layout_flags_and_ands_gpu_only() {
        let mut a = RestyleResult {
            needs_layout: false,
            needs_display_list: false,
            gpu_only_changes: true,
            ..RestyleResult::default()
        };
        let b = RestyleResult {
            needs_layout: true,
            needs_display_list: true,
            gpu_only_changes: true,
            ..RestyleResult::default()
        };
        a.merge(b);
        assert!(a.needs_layout, "needs_layout is OR-ed");
        assert!(a.needs_display_list, "needs_display_list is OR-ed");
        assert!(a.gpu_only_changes, "true && true stays true");

        // ...and a single non-GPU-only participant clears the flag.
        let mut c = RestyleResult {
            gpu_only_changes: true,
            ..RestyleResult::default()
        };
        c.merge(RestyleResult {
            gpu_only_changes: false,
            ..RestyleResult::default()
        });
        assert!(!c.gpu_only_changes, "gpu_only_changes is AND-ed");
    }

    #[test]
    fn restyle_result_merge_keeps_the_most_expensive_scope() {
        let mut low = RestyleResult {
            max_relayout_scope: RelayoutScope::None,
            ..RestyleResult::default()
        };
        low.merge(RestyleResult {
            max_relayout_scope: RelayoutScope::Full,
            ..RestyleResult::default()
        });
        assert_eq!(low.max_relayout_scope, RelayoutScope::Full);

        // ...and merging a cheaper scope must NOT downgrade it.
        let mut high = RestyleResult {
            max_relayout_scope: RelayoutScope::Full,
            ..RestyleResult::default()
        };
        high.merge(RestyleResult {
            max_relayout_scope: RelayoutScope::IfcOnly,
            ..RestyleResult::default()
        });
        assert_eq!(high.max_relayout_scope, RelayoutScope::Full);
    }

    #[test]
    fn restyle_result_merge_of_default_is_not_the_identity_for_gpu_only() {
        // `RestyleResult::default()` has gpu_only_changes == false, and merge()
        // AND-s that flag — so merging an EMPTY result still clears it. Pinned
        // here because it is a genuine footgun for callers that merge in a loop.
        let mut a = RestyleResult {
            gpu_only_changes: true,
            ..RestyleResult::default()
        };
        a.merge(RestyleResult::default());
        assert!(!a.gpu_only_changes);
        assert!(!a.has_changes());
    }

    #[test]
    fn restyle_result_merge_concatenates_changes_for_the_same_node() {
        let prop = |t| ChangedCssProperty {
            previous_state: StyledNodeState::new(),
            previous_prop: CssProperty::auto(t),
            current_state: StyledNodeState::new(),
            current_prop: CssProperty::initial(t),
        };

        let mut a = RestyleResult::default();
        a.changed_nodes
            .insert(NodeId::ZERO, vec![prop(CssPropertyType::Width)]);

        let mut b = RestyleResult::default();
        b.changed_nodes
            .insert(NodeId::ZERO, vec![prop(CssPropertyType::Height)]);
        b.changed_nodes
            .insert(NodeId::new(1), vec![prop(CssPropertyType::Opacity)]);

        a.merge(b);

        assert_eq!(a.changed_nodes.len(), 2);
        assert_eq!(
            a.changed_nodes[&NodeId::ZERO].len(),
            2,
            "changes for the same node are appended, not replaced"
        );
        assert_eq!(a.changed_nodes[&NodeId::new(1)].len(), 1);
        assert!(a.has_changes());
    }

    // ---------------------------------------------------------------------
    // StyledNodeState (constructor + predicates)
    // ---------------------------------------------------------------------

    #[test]
    fn styled_node_state_new_is_all_false_and_normal() {
        let s = StyledNodeState::new();
        assert!(s.is_normal());
        assert!(!s.hover);
        assert!(!s.active);
        assert!(!s.focused);
        assert!(!s.disabled);
        assert!(!s.checked);
        assert!(!s.focus_within);
        assert!(!s.visited);
        assert!(!s.backdrop);
        assert!(!s.dragging);
        assert!(!s.drag_over);
        assert_eq!(s, StyledNodeState::default());
    }

    #[test]
    fn styled_node_state_has_state_zero_is_always_true() {
        // 0 == "Normal", which is active regardless of the other flags.
        assert!(StyledNodeState::new().has_state(0));
        assert!(StyledNodeState::from_pseudo_state_flags(&pseudo_flags(true)).has_state(0));
    }

    #[test]
    fn styled_node_state_has_state_maps_every_index_exactly_once() {
        // Each setter must light up exactly one state index in 1..=10.
        let setters: [(u8, fn(&mut StyledNodeState)); 10] = [
            (1, |s| s.hover = true),
            (2, |s| s.active = true),
            (3, |s| s.focused = true),
            (4, |s| s.disabled = true),
            (5, |s| s.checked = true),
            (6, |s| s.focus_within = true),
            (7, |s| s.visited = true),
            (8, |s| s.backdrop = true),
            (9, |s| s.dragging = true),
            (10, |s| s.drag_over = true),
        ];

        for (expected_idx, set) in setters {
            let mut s = StyledNodeState::new();
            set(&mut s);
            assert!(!s.is_normal(), "state {expected_idx} must not be 'normal'");
            for idx in 1..=10u8 {
                assert_eq!(
                    s.has_state(idx),
                    idx == expected_idx,
                    "state index {idx} misreported for setter {expected_idx}"
                );
            }
        }
    }

    #[test]
    fn styled_node_state_has_state_is_false_for_every_out_of_range_u8() {
        let all_on = StyledNodeState::from_pseudo_state_flags(&pseudo_flags(true));
        for idx in 11..=u8::MAX {
            assert!(!StyledNodeState::new().has_state(idx));
            assert!(
                !all_on.has_state(idx),
                "unknown state index {idx} must be inactive even when every flag is set"
            );
        }
    }

    #[test]
    fn styled_node_state_from_pseudo_state_flags_roundtrips_every_field() {
        let all_on = StyledNodeState::from_pseudo_state_flags(&pseudo_flags(true));
        assert!(!all_on.is_normal());
        for idx in 0..=10u8 {
            assert!(all_on.has_state(idx), "state {idx} should be active");
        }

        let all_off = StyledNodeState::from_pseudo_state_flags(&pseudo_flags(false));
        assert!(all_off.is_normal());
        assert_eq!(all_off, StyledNodeState::new());
    }

    #[test]
    fn styled_node_state_debug_lists_active_states_and_normal_when_empty() {
        assert_eq!(format!("{:?}", StyledNodeState::new()), "[\"normal\"]");

        let mut s = StyledNodeState::new();
        s.hover = true;
        s.drag_over = true;
        let dbg = format!("{s:?}");
        assert!(dbg.contains("hover"), "{dbg}");
        assert!(dbg.contains("drag_over"), "{dbg}");
        assert!(!dbg.contains("normal"), "{dbg}");
    }

    // ---------------------------------------------------------------------
    // StyledNodeVec containers
    // ---------------------------------------------------------------------

    #[test]
    fn styled_node_vec_empty_container_is_empty_and_get_returns_none() {
        let v: StyledNodeVec = Vec::new().into();
        let c = v.as_container();
        assert_eq!(c.len(), 0);
        assert!(c.is_empty());
        assert!(c.get(NodeId::ZERO).is_none());
        assert!(c.get(NodeId::new(usize::MAX)).is_none());
    }

    #[test]
    fn styled_node_vec_container_mut_writes_are_visible_through_container() {
        let mut v: StyledNodeVec = vec![StyledNode::default(), StyledNode::default()].into();
        {
            let mut c = v.as_container_mut();
            c[NodeId::new(1)].styled_node_state.hover = true;
        }
        let c = v.as_container();
        assert_eq!(c.len(), 2);
        assert!(!c[NodeId::ZERO].styled_node_state.hover);
        assert!(c[NodeId::new(1)].styled_node_state.hover);
        assert!(c.get(NodeId::new(2)).is_none());
    }

    // ---------------------------------------------------------------------
    // Font family hashes
    // ---------------------------------------------------------------------

    #[test]
    fn style_font_family_hash_is_deterministic_and_input_sensitive() {
        assert_eq!(
            StyleFontFamilyHash::new(&family("Arial")),
            StyleFontFamilyHash::new(&family("Arial"))
        );
        assert_ne!(
            StyleFontFamilyHash::new(&family("Arial")),
            StyleFontFamilyHash::new(&family("Ariaĺ"))
        );
        // Same string, different variant → different cache key.
        assert_ne!(
            StyleFontFamilyHash::new(&StyleFontFamily::System("x".to_string().into())),
            StyleFontFamilyHash::new(&StyleFontFamily::File("x".to_string().into()))
        );
    }

    #[test]
    fn style_font_family_hash_handles_empty_unicode_and_huge_names() {
        let empty = family("");
        let unicode = family("🦀 ノート ﷽ عربى");
        let huge = family(&"A".repeat(100_000));

        // No panic, and each distinct input is stable across calls.
        assert_eq!(
            StyleFontFamilyHash::new(&empty),
            StyleFontFamilyHash::new(&empty)
        );
        assert_eq!(
            StyleFontFamilyHash::new(&unicode),
            StyleFontFamilyHash::new(&unicode)
        );
        assert_eq!(
            StyleFontFamilyHash::new(&huge),
            StyleFontFamilyHash::new(&huge)
        );
        assert_ne!(
            StyleFontFamilyHash::new(&empty),
            StyleFontFamilyHash::new(&unicode)
        );
        assert_ne!(
            StyleFontFamilyHash::new(&empty),
            StyleFontFamilyHash::new(&huge)
        );
    }

    #[test]
    fn style_font_families_hash_empty_slice_is_stable_and_distinct() {
        let empty = StyleFontFamiliesHash::new(&[]);
        assert_eq!(empty, StyleFontFamiliesHash::new(&[]));
        assert_ne!(empty, StyleFontFamiliesHash::new(&[family("")]));
    }

    #[test]
    fn style_font_families_hash_scales_to_large_lists_and_is_length_sensitive() {
        let big: Vec<StyleFontFamily> = (0..1000).map(|i| family(&format!("font-{i}"))).collect();
        let one_shorter = &big[..999];

        assert_eq!(
            StyleFontFamiliesHash::new(&big),
            StyleFontFamiliesHash::new(&big),
            "hashing 1000 families must be deterministic"
        );
        assert_ne!(
            StyleFontFamiliesHash::new(&big),
            StyleFontFamiliesHash::new(one_shorter),
            "the length prefix must separate [0..1000) from [0..999)"
        );
    }

    // ---------------------------------------------------------------------
    // NodeHierarchyItemId: 1-based encode/decode round-trip
    // ---------------------------------------------------------------------

    #[test]
    fn node_hierarchy_item_id_none_is_zero() {
        assert_eq!(NodeHierarchyItemId::NONE.into_raw(), 0);
        assert_eq!(NodeHierarchyItemId::NONE.into_crate_internal(), None);
        assert_eq!(NodeHierarchyItemId::from_crate_internal(None).into_raw(), 0);
        assert_eq!(NodeHierarchyItemId::from_raw(0).into_crate_internal(), None);
        assert_eq!(
            NodeHierarchyItemId::from_crate_internal(None),
            NodeHierarchyItemId::NONE
        );
    }

    #[test]
    fn node_hierarchy_item_id_encode_decode_roundtrip_at_boundaries() {
        // usize::MAX - 1 is the largest index that survives the +1 encoding.
        for idx in [0usize, 1, 2, 1023, usize::MAX / 2, usize::MAX - 1] {
            let id = NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(idx)));
            assert_eq!(id.into_raw(), idx + 1, "1-based encoding for {idx}");
            assert_eq!(
                id.into_crate_internal(),
                Some(NodeId::new(idx)),
                "decode(encode(x)) == x for {idx}"
            );
        }
    }

    #[test]
    fn node_hierarchy_item_id_raw_roundtrip_is_identity_even_at_usize_max() {
        for raw in [0usize, 1, 2, 7, u32::MAX as usize, usize::MAX] {
            let decoded = NodeHierarchyItemId::from_raw(raw).into_crate_internal();
            let reencoded = NodeHierarchyItemId::from_crate_internal(decoded).into_raw();
            assert_eq!(
                reencoded, raw,
                "encode(decode(raw)) must be identity for {raw}"
            );
        }
    }

    #[test]
    fn node_hierarchy_item_id_from_raw_decodes_one_based() {
        assert_eq!(
            NodeHierarchyItemId::from_raw(1).into_crate_internal(),
            Some(NodeId::ZERO),
            "raw 1 is NodeId(0), NOT NodeId(1)"
        );
        assert_eq!(
            NodeHierarchyItemId::from_raw(usize::MAX).into_crate_internal(),
            Some(NodeId::new(usize::MAX - 1))
        );
    }

    #[test]
    fn node_hierarchy_item_id_debug_and_display_agree() {
        let none = NodeHierarchyItemId::NONE;
        assert_eq!(format!("{none:?}"), "None");
        assert_eq!(format!("{none}"), format!("{none:?}"));

        let some = NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(5)));
        assert_eq!(format!("{some:?}"), "Some(NodeId(5))");
        assert_eq!(format!("{some}"), format!("{some:?}"));

        // Extreme value: must not panic and must stay non-empty.
        let max = NodeHierarchyItemId::from_raw(usize::MAX);
        assert!(!format!("{max:?}").is_empty());
    }

    #[test]
    fn node_hierarchy_item_id_ordering_follows_raw_value() {
        let a = NodeHierarchyItemId::from_raw(0);
        let b = NodeHierarchyItemId::from_raw(1);
        let c = NodeHierarchyItemId::from_raw(usize::MAX);
        assert!(a < b);
        assert!(b < c);
        assert_eq!(a, NodeHierarchyItemId::NONE);
    }

    #[test]
    fn node_hierarchy_item_id_from_impls_match_the_explicit_ones() {
        let opt = Some(NodeId::new(41));
        let via_from: NodeHierarchyItemId = opt.into();
        assert_eq!(via_from, NodeHierarchyItemId::from_crate_internal(opt));

        let back: Option<NodeId> = via_from.into();
        assert_eq!(back, opt);

        let none: NodeHierarchyItemId = None.into();
        assert_eq!(none.into_raw(), 0);
    }

    // ---------------------------------------------------------------------
    // NodeHierarchyItem getters
    // ---------------------------------------------------------------------

    #[test]
    fn node_hierarchy_item_zeroed_has_no_links() {
        let z = NodeHierarchyItem::zeroed();
        assert_eq!(z.parent_id(), None);
        assert_eq!(z.previous_sibling_id(), None);
        assert_eq!(z.next_sibling_id(), None);
        assert_eq!(z.last_child_id(), None);
        assert_eq!(z.first_child_id(NodeId::ZERO), None);
        assert_eq!(z.first_child_id(NodeId::new(usize::MAX)), None);
        assert_eq!(z, NodeHierarchyItem::from(Node::ROOT));
    }

    #[test]
    fn node_hierarchy_item_getters_decode_the_one_based_fields() {
        let item = raw_item(1, 2, 3, 4);
        assert_eq!(item.parent_id(), Some(NodeId::new(0)));
        assert_eq!(item.previous_sibling_id(), Some(NodeId::new(1)));
        assert_eq!(item.next_sibling_id(), Some(NodeId::new(2)));
        assert_eq!(item.last_child_id(), Some(NodeId::new(3)));

        // first_child is derived: parent + 1, but only if the node has children.
        assert_eq!(item.first_child_id(NodeId::new(7)), Some(NodeId::new(8)));
    }

    #[test]
    fn node_hierarchy_item_getters_at_usize_max_do_not_overflow() {
        let item = raw_item(usize::MAX, usize::MAX, usize::MAX, usize::MAX);
        assert_eq!(item.parent_id(), Some(NodeId::new(usize::MAX - 1)));
        assert_eq!(
            item.previous_sibling_id(),
            Some(NodeId::new(usize::MAX - 1))
        );
        assert_eq!(item.next_sibling_id(), Some(NodeId::new(usize::MAX - 1)));
        assert_eq!(item.last_child_id(), Some(NodeId::new(usize::MAX - 1)));

        // NodeId's Add is saturating, so `current + 1` clamps instead of wrapping
        // to 0 (which would alias the root node).
        assert_eq!(
            item.first_child_id(NodeId::new(usize::MAX)),
            Some(NodeId::new(usize::MAX)),
            "first_child_id must saturate, never wrap to NodeId(0)"
        );
    }

    #[test]
    fn node_hierarchy_item_from_node_preserves_every_link() {
        let node = Node {
            parent: Some(NodeId::new(3)),
            previous_sibling: None,
            next_sibling: Some(NodeId::new(9)),
            last_child: Some(NodeId::new(12)),
        };
        let item: NodeHierarchyItem = node.into();
        assert_eq!(item.parent_id(), node.parent);
        assert_eq!(item.previous_sibling_id(), node.previous_sibling);
        assert_eq!(item.next_sibling_id(), node.next_sibling);
        assert_eq!(item.last_child_id(), node.last_child);
    }

    // ---------------------------------------------------------------------
    // NodeHierarchyItemVec container + subtree_len
    // ---------------------------------------------------------------------

    #[test]
    fn node_hierarchy_item_vec_containers_read_and_write() {
        let mut v: NodeHierarchyItemVec = vec![NodeHierarchyItem::zeroed(); 2].into();
        {
            let mut c = v.as_container_mut();
            c[NodeId::new(1)].parent = 1; // raw 1 == NodeId(0)
        }
        let c = v.as_container();
        assert_eq!(c.len(), 2);
        assert_eq!(c[NodeId::new(1)].parent_id(), Some(NodeId::ZERO));
        assert!(c.get(NodeId::new(2)).is_none());

        let empty: NodeHierarchyItemVec = Vec::new().into();
        assert!(empty.as_container().is_empty());
    }

    #[test]
    fn subtree_len_counts_descendants_of_a_real_tree() {
        // body(0) > div(1) > div(2)
        let sd = nested_body();
        let h = sd.node_hierarchy.as_container();
        assert_eq!(h.len(), 3);
        assert_eq!(h.subtree_len(NodeId::ZERO), 2, "root has 2 descendants");
        assert_eq!(h.subtree_len(NodeId::new(1)), 1);
        assert_eq!(
            h.subtree_len(NodeId::new(2)),
            0,
            "a leaf has no descendants"
        );
    }

    #[test]
    fn subtree_len_saturates_on_a_malformed_backwards_next_sibling() {
        // Node 2 claims its next sibling is node 0 — a backwards link a malformed
        // FastDom can produce. The subtraction must saturate, not underflow-panic.
        let v: NodeHierarchyItemVec = vec![
            raw_item(0, 0, 0, 0),
            raw_item(0, 0, 0, 0),
            raw_item(0, 0, /* next = NodeId(0) */ 1, 0),
        ]
        .into();
        let c = v.as_container();
        assert_eq!(c.subtree_len(NodeId::new(2)), 0);

        // Self-referential next_sibling (node 1 -> node 1) must also saturate.
        let v2: NodeHierarchyItemVec = vec![raw_item(0, 0, 0, 0), raw_item(0, 0, 2, 0)].into();
        assert_eq!(v2.as_container().subtree_len(NodeId::new(1)), 0);
    }

    // ---------------------------------------------------------------------
    // StyledDomMemoryReport
    // ---------------------------------------------------------------------

    #[test]
    fn memory_report_default_total_is_zero() {
        assert_eq!(StyledDomMemoryReport::default().total_bytes(), 0);
    }

    #[test]
    fn memory_report_total_bytes_sums_every_field() {
        let r = StyledDomMemoryReport {
            node_count: 3,
            node_hierarchy_bytes: 1,
            node_data_bytes: 2,
            styled_nodes_bytes: 4,
            cascade_info_bytes: 8,
            tag_ids_bytes: 16,
            non_leaf_nodes_bytes: 32,
            callback_vecs_bytes: 64,
            ..StyledDomMemoryReport::default()
        };
        assert_eq!(
            r.total_bytes(),
            127,
            "node_count must NOT be part of the sum"
        );

        // A single saturated field must not overflow the running sum.
        let extreme = StyledDomMemoryReport {
            node_data_bytes: usize::MAX,
            ..StyledDomMemoryReport::default()
        };
        assert_eq!(extreme.total_bytes(), usize::MAX);
    }

    #[test]
    fn memory_report_tracks_node_count_and_is_monotonic_in_dom_size() {
        let small = flat_body(1).memory_report();
        let large = flat_body(50).memory_report();
        assert_eq!(small.node_count, 2);
        assert_eq!(large.node_count, 51);
        assert!(large.total_bytes() > small.total_bytes());
        assert!(small.total_bytes() >= small.node_hierarchy_bytes + small.node_data_bytes);

        // Also fine on the smallest possible DOM.
        let d = StyledDom::default().memory_report();
        assert_eq!(d.node_count, 1);
        assert!(d.total_bytes() > 0);
    }

    // ---------------------------------------------------------------------
    // StyledDom construction
    // ---------------------------------------------------------------------

    #[test]
    fn default_styled_dom_is_a_single_rooted_body() {
        let sd = StyledDom::default();
        assert_eq!(sd.node_count(), 1);
        assert_eq!(sd.root.into_crate_internal(), Some(NodeId::ZERO));
        assert_eq!(sd.node_hierarchy.as_ref().len(), 1);
        assert_eq!(sd.styled_nodes.as_ref().len(), 1);
        assert_eq!(sd.cascade_info.as_ref().len(), 1);
        assert_eq!(sd.non_leaf_nodes.as_ref().len(), 1);
        assert_eq!(sd.non_leaf_nodes.as_ref()[0].depth, 0);
        assert!(sd.tag_ids_to_node_ids.as_ref().is_empty());
        assert!(sd.get_styled_node_state(&NodeId::ZERO).is_normal());
    }

    /// miniword ENGINE-ISSUE 4: `Dom::create_text_do_not_use_without_block_level_wrapper(..).with_css(..)` silently
    /// dropped EVERY declaration — the bare-decl wrapper parses to
    /// `* { .. }`, and the `Global` matcher refused text nodes even for
    /// rules scoped to exactly that node. All four reported strings now
    /// cascade onto the text node.
    #[test]
    fn with_css_on_a_text_node_applies_its_declarations() {
        use azul_css::props::basic::color::ColorU;

        let cases: &[(&str, ColorU, isize)] = &[
            (
                "font-size: 38px; color: #565656;",
                ColorU {
                    r: 0x56,
                    g: 0x56,
                    b: 0x56,
                    a: 255,
                },
                38,
            ),
            (
                "font-size: 38px; color: #565656; flex-grow: 0;",
                ColorU {
                    r: 0x56,
                    g: 0x56,
                    b: 0x56,
                    a: 255,
                },
                38,
            ),
            (
                "font-size: 16px; color: #2b579a; margin-bottom: 12px;",
                ColorU {
                    r: 0x2b,
                    g: 0x57,
                    b: 0x9a,
                    a: 255,
                },
                16,
            ),
            (
                "font-size: 13px; color: white;",
                ColorU {
                    r: 255,
                    g: 255,
                    b: 255,
                    a: 255,
                },
                13,
            ),
        ];

        for (css_str, want_color, want_px) in cases {
            let dom = crate::dom::Dom::create_body().with_child(
                crate::dom::Dom::create_div()
                    .with_css("color: #444444; font-size: 10px;")
                    .with_child(
                        crate::dom::Dom::create_text_do_not_use_without_block_level_wrapper("X")
                            .with_css(css_str),
                    ),
            );
            // create_from_dom is the production path (scope_inline_css +
            // collect_css_from_dom); plain create() ignores dom.css.
            let styled = StyledDom::create_from_dom(dom);
            let cache = styled.get_css_property_cache();
            let n = styled.node_data.as_ref().len() - 1;
            let node_id = NodeId::new(n);
            let node_data = &styled.node_data.as_ref()[n];
            assert!(
                node_data.is_text_node(),
                "fixture: last node must be the text node"
            );
            let state = &styled.styled_nodes.as_ref()[n].styled_node_state;

            let color = cache
                .get_text_color(node_data, &node_id, state)
                .and_then(|p| p.get_property().copied())
                .map(|c| c.inner);
            assert_eq!(
                color,
                Some(*want_color),
                "inline color lost on text node for {css_str:?}"
            );
            let size = cache
                .get_font_size(node_data, &node_id, state)
                .and_then(|p| p.get_property().copied())
                .map(|s| s.inner.to_pixels_internal(16.0, 16.0, 16.0) as isize);
            assert_eq!(
                size,
                Some(*want_px),
                "inline font-size lost on text node for {css_str:?}"
            );
        }

        // Negative control: a text node WITHOUT inline css keeps taking its
        // color by INHERITANCE (the parent's #444444 arrives via
        // cascaded_props) — the matcher exception must not have rerouted or
        // broken the inheritance lane.
        let dom = crate::dom::Dom::create_body().with_child(
            crate::dom::Dom::create_div()
                .with_css("color: #444444;")
                .with_child(
                    crate::dom::Dom::create_text_do_not_use_without_block_level_wrapper("X"),
                ),
        );
        let styled = StyledDom::create_from_dom(dom);
        let cache = styled.get_css_property_cache();
        let n = styled.node_data.as_ref().len() - 1;
        let node_id = NodeId::new(n);
        let node_data = &styled.node_data.as_ref()[n];
        let state = &styled.styled_nodes.as_ref()[n].styled_node_state;
        assert!(node_data.is_text_node());
        assert!(
            cache.css_props.get_slice(n).is_empty(),
            "an unstyled text node must have no OWN css_props"
        );
        let inherited = cache
            .get_text_color(node_data, &node_id, state)
            .and_then(|p| p.get_property().copied())
            .map(|c| c.inner);
        assert_eq!(
            inherited,
            Some(ColorU {
                r: 0x44,
                g: 0x44,
                b: 0x44,
                a: 255
            }),
            "inheritance must still deliver the parent's color to the text node"
        );
    }

    #[test]
    fn create_empties_the_source_dom() {
        // Documented: "After calling this function, the DOM will be reset to an empty DOM."
        let mut dom = Dom::create_body().with_children(vec![Dom::create_div(); 3].into());
        let sd = StyledDom::create(&mut dom, Css::empty());
        assert_eq!(sd.node_count(), 4);
        assert!(
            dom.children.as_ref().is_empty(),
            "the source Dom must be left empty (it is swapped out, not cloned)"
        );
    }

    #[test]
    fn create_keeps_every_parallel_array_the_same_length() {
        for n in [0usize, 1, 3, 64] {
            let sd = flat_body(n);
            let count = sd.node_count();
            assert_eq!(count, n + 1);
            assert_eq!(sd.node_hierarchy.as_ref().len(), count);
            assert_eq!(sd.styled_nodes.as_ref().len(), count);
            assert_eq!(sd.cascade_info.as_ref().len(), count);
        }
    }

    #[test]
    fn create_survives_malformed_truncated_and_unicode_css() {
        let cases: Vec<String> = vec![
            String::new(),
            "}}}{{{".to_string(),
            "div {".to_string(),
            "div { color: }".to_string(),
            "div { : red; }".to_string(),
            "@media".to_string(),
            "/* unterminated comment".to_string(),
            "div { width: 99999999999999999999999px; }".to_string(),
            "div { width: -0px; opacity: 1e400; }".to_string(),
            "div { width: NaNpx; height: infpx; }".to_string(),
            "* { color: #ZZZZZZ; }".to_string(),
            "日本語 { content: \"🦀\"; }".to_string(),
            ".\u{202e}rtl { color: red; }".to_string(),
            "a".repeat(10_000),
            "div { color: red; }".repeat(500),
        ];

        for case in &cases {
            let css = parse_css(case);
            let mut dom = Dom::create_body().with_children(vec![Dom::create_div()].into());
            let sd = StyledDom::create(&mut dom, css);
            assert_eq!(
                sd.node_count(),
                2,
                "CSS must never change the node count; failing input: {case:?}"
            );
        }
    }

    #[test]
    fn create_handles_deep_and_wide_doms() {
        // deep: 64 nested divs under a body
        let mut deep = Dom::create_div();
        for _ in 0..63 {
            deep = Dom::create_div().with_children(vec![deep].into());
        }
        let mut deep_body = Dom::create_body().with_children(vec![deep].into());
        let sd = StyledDom::create(&mut deep_body, Css::empty());
        assert_eq!(sd.node_count(), 65);
        assert_eq!(
            sd.non_leaf_nodes.as_ref().len(),
            64,
            "every node except the innermost leaf is a parent"
        );

        // wide: 1000 siblings
        let wide = flat_body(1000);
        assert_eq!(wide.node_count(), 1001);
        assert_eq!(
            wide.node_hierarchy.as_container().subtree_len(NodeId::ZERO),
            1000
        );
        assert_eq!(wide.non_leaf_nodes.as_ref().len(), 1);
    }

    #[test]
    fn create_from_dom_collects_scoped_css_without_changing_the_tree() {
        let dom = Dom::create_body().with_children(
            vec![
                Dom::create_div().with_css("color: red"),
                Dom::create_div()
                    .with_children(vec![Dom::create_div().with_css("width: 5px")].into()),
            ]
            .into(),
        );
        let sd = StyledDom::create_from_dom(dom);
        assert_eq!(sd.node_count(), 4);
        assert_eq!(sd.node_hierarchy.as_ref().len(), 4);
        assert!(sd.get_css_property_cache().compact_cache.is_some());
    }

    #[test]
    fn create_from_dom_on_a_bare_leaf_produces_one_node() {
        let sd = StyledDom::create_from_dom(Dom::create_div());
        assert_eq!(sd.node_count(), 1);
        assert_eq!(sd.root.into_crate_internal(), Some(NodeId::ZERO));
    }

    // ---------------------------------------------------------------------
    // append_child / append_child_with_index / finalize / with_child
    // ---------------------------------------------------------------------

    #[test]
    fn append_child_grows_the_node_count_by_the_child_dom_size() {
        let mut base = flat_body(2);
        base.append_child(flat_body(3));
        assert_eq!(base.node_count(), 3 + 4);
        assert_eq!(base.node_hierarchy.as_ref().len(), 7);
        assert_eq!(base.styled_nodes.as_ref().len(), 7);
        assert_eq!(base.cascade_info.as_ref().len(), 7);
    }

    #[test]
    fn append_child_links_the_new_root_as_the_last_sibling() {
        // Flat parent: body(0) > [div(1), div(2)], then append a 1-node StyledDom.
        let mut base = flat_body(2);
        base.append_child(StyledDom::default());

        let h = base.node_hierarchy.as_container();
        let children: Vec<NodeId> = NodeId::ZERO.az_children(&h).collect();
        assert_eq!(
            children,
            vec![NodeId::new(1), NodeId::new(2), NodeId::new(3)],
            "the appended root must become the last direct child"
        );
        assert_eq!(h[NodeId::new(3)].parent_id(), Some(NodeId::ZERO));
        assert_eq!(
            h[NodeId::new(3)].previous_sibling_id(),
            Some(NodeId::new(2))
        );
        assert_eq!(h[NodeId::new(3)].next_sibling_id(), None);
    }

    /// ADVERSARIAL: `append_child` reads `last_child_id()` to find the current
    /// last sibling. If `last_child` names a *descendant* rather than the last
    /// *direct child*, the appended root is spliced into the wrong sibling chain
    /// and disappears from the root's children.
    #[test]
    fn append_child_keeps_the_root_children_reachable_for_a_nested_dom() {
        let mut base = nested_body(); // body(0) > div(1) > div(2)
        base.append_child(StyledDom::default());
        assert_eq!(base.node_count(), 4);

        let h = base.node_hierarchy.as_container();
        let children: Vec<NodeId> = NodeId::ZERO.az_children(&h).collect();
        assert_eq!(
            children,
            vec![NodeId::new(1), NodeId::new(3)],
            "after append_child the root must have exactly its old child plus the appended root"
        );
    }

    #[test]
    fn append_child_with_index_saturates_the_u32_cascade_index() {
        for (child_index, expected) in [
            (0usize, 0u32),
            (7, 7),
            (u32::MAX as usize, u32::MAX),
            (u32::MAX as usize + 1, u32::MAX),
            (usize::MAX, u32::MAX),
        ] {
            let mut base = flat_body(0); // single body node
            base.append_child_with_index(StyledDom::default(), child_index);

            // The appended root lands at index self_len == 1 in the merged arrays.
            assert_eq!(
                base.cascade_info.as_ref()[1].index_in_parent,
                expected,
                "child_index {child_index} must saturate to {expected}, never wrap"
            );
            assert!(base.cascade_info.as_ref()[1].is_last_child);
            assert_eq!(base.node_count(), 2);
        }
    }

    #[test]
    fn finalize_non_leaf_nodes_sorts_by_depth_and_is_idempotent() {
        let mut base = flat_body(1);
        base.append_child_with_index(flat_body(2), 1);
        base.append_child_with_index(flat_body(2), 2);
        base.finalize_non_leaf_nodes();

        let depths: Vec<usize> = base
            .non_leaf_nodes
            .as_ref()
            .iter()
            .map(|p| p.depth)
            .collect();
        let mut sorted = depths.clone();
        sorted.sort_unstable();
        assert_eq!(depths, sorted, "non_leaf_nodes must be depth-ordered");

        base.finalize_non_leaf_nodes();
        let again: Vec<usize> = base
            .non_leaf_nodes
            .as_ref()
            .iter()
            .map(|p| p.depth)
            .collect();
        assert_eq!(depths, again, "finalize must be idempotent");
    }

    #[test]
    fn with_child_matches_append_child() {
        let mut appended = flat_body(2);
        appended.append_child(flat_body(1));

        let built = flat_body(2).with_child(flat_body(1));

        assert_eq!(built.node_count(), appended.node_count());
        assert_eq!(
            built.node_hierarchy.as_ref(),
            appended.node_hierarchy.as_ref()
        );
    }

    #[test]
    fn swap_with_default_returns_the_old_dom_and_resets_self() {
        let mut sd = flat_body(3);
        let old = sd.swap_with_default();
        assert_eq!(old.node_count(), 4);
        assert_eq!(
            sd.node_count(),
            1,
            "self must be left as the default StyledDom"
        );
        assert_eq!(sd.root.into_crate_internal(), Some(NodeId::ZERO));
    }

    // ---------------------------------------------------------------------
    // Menus
    // ---------------------------------------------------------------------

    #[test]
    fn context_menu_and_menu_bar_are_stored_on_the_root_node() {
        let mut sd = flat_body(1);
        assert!(sd.node_data.as_container()[NodeId::ZERO]
            .get_context_menu()
            .is_none());

        sd.set_context_menu(empty_menu());
        sd.set_menu_bar(empty_menu());

        let data = sd.node_data.as_container();
        assert!(data[NodeId::ZERO].get_context_menu().is_some());
        assert!(data[NodeId::ZERO].get_menu_bar().is_some());

        // ...and the child must not have inherited either of them.
        assert!(data[NodeId::new(1)].get_context_menu().is_none());
        assert!(data[NodeId::new(1)].get_menu_bar().is_none());
    }

    #[test]
    fn menu_builders_are_equivalent_to_the_setters_and_dont_touch_the_tree() {
        let sd = StyledDom::default()
            .with_context_menu(empty_menu())
            .with_menu_bar(empty_menu());
        assert_eq!(sd.node_count(), 1);
        let data = sd.node_data.as_container();
        assert!(data[NodeId::ZERO].get_context_menu().is_some());
        assert!(data[NodeId::ZERO].get_menu_bar().is_some());
    }

    // ---------------------------------------------------------------------
    // restyle_nodes_* / restyle_on_state_change / restyle_user_property
    // ---------------------------------------------------------------------

    #[test]
    fn restyle_nodes_hover_sets_and_clears_the_state_flag() {
        let mut sd = flat_body(2);
        let _ = sd.restyle_nodes_hover(&[NodeId::new(1)], true);
        assert!(sd.get_styled_node_state(&NodeId::new(1)).hover);
        assert!(!sd.get_styled_node_state(&NodeId::new(2)).hover);

        let _ = sd.restyle_nodes_hover(&[NodeId::new(1)], false);
        assert!(!sd.get_styled_node_state(&NodeId::new(1)).hover);
        assert!(sd.get_styled_node_state(&NodeId::new(1)).is_normal());
    }

    #[test]
    fn restyle_nodes_active_and_focus_set_independent_flags() {
        let mut sd = flat_body(1);
        let _ = sd.restyle_nodes_active(&[NodeId::ZERO], true);
        let _ = sd.restyle_nodes_focus(&[NodeId::ZERO], true);

        let state = sd.get_styled_node_state(&NodeId::ZERO);
        assert!(state.active);
        assert!(state.focused);
        assert!(!state.hover, "hover must be untouched");
        assert!(!state.is_normal());
    }

    #[test]
    fn restyle_nodes_ignores_out_of_range_node_ids_instead_of_panicking() {
        let mut sd = flat_body(1); // valid ids: 0, 1
        let changed = sd.restyle_nodes_hover(&[NodeId::new(2), NodeId::new(usize::MAX)], true);
        assert!(changed.is_empty());
        assert!(!sd.get_styled_node_state(&NodeId::ZERO).hover);
        assert!(!sd.get_styled_node_state(&NodeId::new(1)).hover);

        // A mix of valid and stale ids must still apply the valid ones.
        let _ = sd.restyle_nodes_hover(&[NodeId::new(1), NodeId::new(999)], true);
        assert!(sd.get_styled_node_state(&NodeId::new(1)).hover);
    }

    #[test]
    fn restyle_nodes_handles_empty_and_duplicated_input() {
        let mut sd = flat_body(1);
        assert!(sd.restyle_nodes_focus(&[], true).is_empty());

        // Duplicates must be idempotent, not double-applied or panicking.
        let _ = sd.restyle_nodes_focus(&[NodeId::ZERO, NodeId::ZERO, NodeId::ZERO], true);
        assert!(sd.get_styled_node_state(&NodeId::ZERO).focused);
    }

    #[test]
    #[should_panic(expected = "index out of bounds")]
    fn get_styled_node_state_panics_on_an_out_of_range_node_id() {
        // Documents the contract: unlike restyle_nodes_*, this getter does NOT
        // bounds-check — callers must pass an id that indexes into this DOM.
        let sd = flat_body(1);
        let _ = sd.get_styled_node_state(&NodeId::new(99));
    }

    #[test]
    fn restyle_on_state_change_with_no_changes_reports_nothing_to_do() {
        let mut sd = flat_body(2);
        let r = sd.restyle_on_state_change(None, None, None);
        assert!(!r.has_changes());
        assert!(!r.needs_layout);
        assert!(!r.needs_display_list);
        assert!(!r.gpu_only_changes);
        assert_eq!(r.max_relayout_scope, RelayoutScope::None);
    }

    #[test]
    fn restyle_on_state_change_tolerates_stale_node_ids() {
        let mut sd = flat_body(1);
        let r = sd.restyle_on_state_change(
            Some(FocusChange {
                lost_focus: Some(NodeId::new(500)),
                gained_focus: Some(NodeId::new(usize::MAX)),
            }),
            Some(HoverChange {
                left_nodes: vec![NodeId::new(700)],
                entered_nodes: vec![NodeId::new(800)],
            }),
            Some(ActiveChange {
                deactivated: vec![NodeId::new(900)],
                activated: vec![NodeId::new(1000)],
            }),
        );
        assert!(!r.has_changes(), "stale ids must be filtered, not applied");
        assert_eq!(sd.node_count(), 2);
    }

    #[test]
    fn restyle_on_state_change_applies_state_to_valid_nodes() {
        let mut sd = flat_body(1);
        let r = sd.restyle_on_state_change(
            None,
            Some(HoverChange {
                left_nodes: Vec::new(),
                entered_nodes: vec![NodeId::new(1)],
            }),
            None,
        );
        assert!(sd.get_styled_node_state(&NodeId::new(1)).hover);
        assert!(
            r.changed_nodes.keys().all(|n| *n == NodeId::new(1)),
            "only the node whose state actually changed may be reported"
        );
    }

    /// A geometry patch must leave the compact cache PRESENT and already
    /// reflecting the override. The old behaviour (drop the cache, "the next
    /// full cascade rebuilds it") broke every consumer that derives state
    /// from the cache during the very relayout that applies the patch: the
    /// font phase resolved an EMPTY font world (no stack signature, no
    /// chains, empty GC keep-set), so the patched-in subtree's text laid out
    /// at zero size and, pre-guard, the font GC evicted every loaded font
    /// mid-frame.
    #[test]
    fn restyle_user_property_rebuilds_the_compact_cache_with_the_patch() {
        use azul_css::props::layout::display::LayoutDisplay;
        use azul_css::props::property::CssProperty;

        let mut sd = flat_body(2);
        let node = NodeId::new(1);

        // Sanity: the fixture starts with a compact cache.
        assert!(
            sd.get_css_property_cache().compact_cache.is_some(),
            "fixture should carry a compact cache"
        );

        let changes =
            sd.restyle_user_property(&node, &[CssProperty::const_display(LayoutDisplay::None)]);
        assert!(
            !changes.is_empty(),
            "display default -> none must report a change"
        );

        let cc = sd
            .get_css_property_cache()
            .compact_cache
            .as_ref()
            .expect("compact cache must be REBUILT by a geometry patch, not dropped");
        assert_eq!(
            cc.get_display(node.index()),
            LayoutDisplay::None,
            "the rebuilt compact cache must already reflect the patched value"
        );

        // And back again - repeated patches keep rebuilding, not accumulating.
        let _ = sd.restyle_user_property(&node, &[CssProperty::const_display(LayoutDisplay::Flex)]);
        let cc = sd
            .get_css_property_cache()
            .compact_cache
            .as_ref()
            .expect("second patch keeps the cache present");
        assert_eq!(cc.get_display(node.index()), LayoutDisplay::Flex);
    }

    #[test]
    fn restyle_user_property_rejects_empty_lists_and_stale_nodes() {
        let mut sd = flat_body(1);
        assert!(sd.restyle_user_property(&NodeId::ZERO, &[]).is_empty());
        assert!(
            sd.restyle_user_property(
                &NodeId::new(50),
                &[CssProperty::auto(CssPropertyType::Width)]
            )
            .is_empty(),
            "an out-of-range node id must be a no-op, not a panic"
        );
        assert!(
            sd.get_css_property_cache()
                .user_overridden_properties
                .iter()
                .all(Vec::is_empty),
            "a rejected call must not record an override"
        );
    }

    #[test]
    fn restyle_user_property_stores_the_override_and_initial_removes_it() {
        let mut sd = flat_body(1);
        let node = NodeId::ZERO;

        let _ = sd.restyle_user_property(&node, &[CssProperty::auto(CssPropertyType::Width)]);
        {
            let overrides = &sd.get_css_property_cache().user_overridden_properties;
            assert_eq!(
                overrides.len(),
                sd.node_count(),
                "table grows to cover the DOM"
            );
            assert_eq!(overrides[0].len(), 1);
            assert_eq!(overrides[0][0].0, CssPropertyType::Width);
        }

        // Re-setting the same type replaces rather than duplicating.
        let _ = sd.restyle_user_property(&node, &[CssProperty::none(CssPropertyType::Width)]);
        assert_eq!(
            sd.get_css_property_cache().user_overridden_properties[0].len(),
            1
        );

        // CssProperty::Initial removes the override again.
        let _ = sd.restyle_user_property(&node, &[CssProperty::initial(CssPropertyType::Width)]);
        assert!(sd.get_css_property_cache().user_overridden_properties[0].is_empty());

        // Removing a property that was never set must not panic.
        let _ = sd.restyle_user_property(&node, &[CssProperty::initial(CssPropertyType::Height)]);
        assert!(sd.get_css_property_cache().user_overridden_properties[0].is_empty());
    }

    #[test]
    fn restyle_and_recompute_preserve_the_tree_and_rebuild_the_compact_cache() {
        let mut sd = flat_body(3);
        let before = sd.node_count();

        sd.restyle(parse_css(
            "div { color: red; } body > div:hover { color: blue; }",
        ));
        assert_eq!(sd.node_count(), before);
        assert!(sd.get_css_property_cache().compact_cache.is_some());

        // A second restyle with garbage CSS must not corrupt the structure.
        sd.restyle(parse_css("}}} div { : ; }"));
        assert_eq!(sd.node_count(), before);

        sd.recompute_inheritance_and_compact_cache();
        assert_eq!(sd.node_count(), before);
        assert!(sd.get_css_property_cache().compact_cache.is_some());
    }

    #[test]
    fn get_css_property_cache_mut_sees_the_same_cache_as_the_shared_getter() {
        let mut sd = flat_body(1);
        let node_count = sd.node_count();
        sd.get_css_property_cache_mut()
            .user_overridden_properties
            .resize(node_count, Vec::new());
        assert_eq!(
            sd.get_css_property_cache().user_overridden_properties.len(),
            node_count
        );
    }

    // ---------------------------------------------------------------------
    // get_html_string
    // ---------------------------------------------------------------------

    #[test]
    fn get_html_string_test_mode_omits_the_html_wrapper() {
        let sd = flat_body(2);
        let out = sd.get_html_string("HEAD_MARK", "BODY_MARK", true);
        assert!(!out.is_empty());
        assert!(
            !out.contains("HEAD_MARK"),
            "test_mode must not emit the custom head"
        );
        assert!(
            !out.contains("BODY_MARK"),
            "test_mode must not emit the custom body"
        );
        assert!(!out.contains("<html>"));
    }

    #[test]
    fn get_html_string_embeds_custom_head_and_body_verbatim() {
        let sd = flat_body(1);
        let head = "🦀 <meta charset=\"utf-8\"> & ünïcödé";
        let body = "x".repeat(10_000);
        let out = sd.get_html_string(head, &body, false);
        assert!(out.contains("<html>"));
        assert!(out.contains(head));
        assert!(out.contains(&body));
    }

    #[test]
    fn get_html_string_does_not_panic_on_extreme_doms() {
        // A single-node DOM has no non_leaf parent entry for its root — the depth
        // lookup must fall back to 0 rather than panic-indexing the map.
        assert!(!StyledDom::default()
            .get_html_string("", "", true)
            .is_empty());
        assert!(!flat_body(0).get_html_string("", "", true).is_empty());
        assert!(!nested_body().get_html_string("", "", true).is_empty());
        assert!(!flat_body(200).get_html_string("", "", true).is_empty());
    }

    // ---------------------------------------------------------------------
    // rendering order
    // ---------------------------------------------------------------------

    #[test]
    fn get_rects_in_rendering_order_is_a_permutation_of_the_children() {
        let sd = flat_body(3);
        let group = sd.get_rects_in_rendering_order();
        assert_eq!(group.root.into_crate_internal(), Some(NodeId::ZERO));

        let mut ids: Vec<usize> = group
            .children
            .as_ref()
            .iter()
            .filter_map(|c| c.root.into_crate_internal())
            .map(|n| n.index())
            .collect();
        ids.sort_unstable();
        assert_eq!(ids, vec![1, 2, 3], "every child appears exactly once");
    }

    #[test]
    fn get_rects_in_rendering_order_nests_grandchildren() {
        let sd = nested_body(); // body(0) > div(1) > div(2)
        let group = sd.get_rects_in_rendering_order();
        assert_eq!(group.children.as_ref().len(), 1);

        let child = &group.children.as_ref()[0];
        assert_eq!(child.root.into_crate_internal(), Some(NodeId::new(1)));
        assert_eq!(child.children.as_ref().len(), 1);
        assert_eq!(
            child.children.as_ref()[0].root.into_crate_internal(),
            Some(NodeId::new(2))
        );
    }

    #[test]
    fn determine_rendering_order_with_no_parents_yields_a_childless_root() {
        let sd = StyledDom::default();
        let hierarchy = sd.node_hierarchy.as_container();
        let styled = sd.styled_nodes.as_container();
        let data = sd.node_data.as_container();

        let group = StyledDom::determine_rendering_order(
            &[],
            &hierarchy,
            &styled,
            &data,
            sd.get_css_property_cache(),
        );
        assert_eq!(group.root.into_crate_internal(), Some(NodeId::ZERO));
        assert!(group.children.as_ref().is_empty());
    }

    #[test]
    fn sort_children_by_position_returns_every_child_of_a_leaf_free_parent() {
        let sd = flat_body(3);
        let hierarchy = sd.node_hierarchy.as_container();
        let styled = sd.styled_nodes.as_container();
        let data = sd.node_data.as_container();

        let sorted = sort_children_by_position(
            NodeId::ZERO,
            &hierarchy,
            &styled,
            &data,
            sd.get_css_property_cache(),
        );
        assert_eq!(sorted.len(), 3);

        // A leaf parent has no children at all.
        let leaf = sort_children_by_position(
            NodeId::new(3),
            &hierarchy,
            &styled,
            &data,
            sd.get_css_property_cache(),
        );
        assert!(leaf.is_empty());
    }

    #[test]
    fn fill_content_group_children_builds_the_nested_group_tree() {
        let id = |i: usize| NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(i)));

        let mut sorted: BTreeMap<NodeHierarchyItemId, Vec<NodeHierarchyItemId>> = BTreeMap::new();
        sorted.insert(id(0), vec![id(1), id(2)]);
        sorted.insert(id(1), vec![id(3)]);

        let mut group = ContentGroup {
            root: id(0),
            children: Vec::new().into(),
        };
        fill_content_group_children(&mut group, &sorted);

        assert_eq!(group.children.as_ref().len(), 2);
        assert_eq!(group.children.as_ref()[0].root, id(1));
        assert_eq!(group.children.as_ref()[0].children.as_ref().len(), 1);
        assert_eq!(group.children.as_ref()[0].children.as_ref()[0].root, id(3));
        assert!(
            group.children.as_ref()[1].children.as_ref().is_empty(),
            "a node with no entry in the map is a leaf"
        );
    }

    #[test]
    fn fill_content_group_children_leaves_an_unknown_root_untouched() {
        let sorted: BTreeMap<NodeHierarchyItemId, Vec<NodeHierarchyItemId>> = BTreeMap::new();
        let mut group = ContentGroup {
            root: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(9))),
            children: Vec::new().into(),
        };
        fill_content_group_children(&mut group, &sorted);
        assert!(group.children.as_ref().is_empty());
    }

    // ---------------------------------------------------------------------
    // recursive_get_last_child / get_path_to_root
    // ---------------------------------------------------------------------

    #[test]
    fn recursive_get_last_child_descends_to_the_deepest_last_child() {
        // 0 -> 1 -> 2 (2 is a leaf)
        let items = vec![
            raw_item(0, 0, 0, 2), // node 0, last_child = NodeId(1)
            raw_item(1, 0, 0, 3), // node 1, last_child = NodeId(2)
            raw_item(2, 0, 0, 0), // node 2, leaf
        ];

        let mut target = None;
        recursive_get_last_child(NodeId::ZERO, &items, &mut target);
        assert_eq!(target, Some(NodeId::new(2)));
    }

    #[test]
    fn recursive_get_last_child_leaves_the_target_untouched_for_a_leaf() {
        let items = vec![raw_item(0, 0, 0, 0)];
        let mut target = None;
        recursive_get_last_child(NodeId::ZERO, &items, &mut target);
        assert_eq!(target, None);

        // A pre-set target is also left alone.
        let mut preset = Some(NodeId::new(7));
        recursive_get_last_child(NodeId::ZERO, &items, &mut preset);
        assert_eq!(preset, Some(NodeId::new(7)));
    }

    #[test]
    fn get_path_to_root_is_root_first_and_tolerates_unknown_nodes() {
        let sd = nested_body(); // body(0) > div(1) > div(2)
        let h = sd.node_hierarchy.as_container();

        assert_eq!(get_path_to_root(&h, NodeId::ZERO), vec![NodeId::ZERO]);
        assert_eq!(
            get_path_to_root(&h, NodeId::new(2)),
            vec![NodeId::ZERO, NodeId::new(1), NodeId::new(2)]
        );

        // An id outside the arena yields a one-element path instead of panicking.
        assert_eq!(
            get_path_to_root(&h, NodeId::new(9999)),
            vec![NodeId::new(9999)]
        );
    }

    // ---------------------------------------------------------------------
    // document order
    // ---------------------------------------------------------------------

    #[test]
    fn is_before_in_document_order_is_false_for_identical_nodes() {
        let sd = flat_body(2);
        assert!(!is_before_in_document_order(
            &sd.node_hierarchy,
            NodeId::new(1),
            NodeId::new(1)
        ));
    }

    #[test]
    fn is_before_in_document_order_orders_ancestors_and_siblings() {
        let sd = flat_body(3); // body(0) > [1, 2, 3]
        let h = &sd.node_hierarchy;

        assert!(is_before_in_document_order(h, NodeId::ZERO, NodeId::new(1)));
        assert!(!is_before_in_document_order(
            h,
            NodeId::new(1),
            NodeId::ZERO
        ));
        assert!(is_before_in_document_order(
            h,
            NodeId::new(1),
            NodeId::new(3)
        ));
        assert!(!is_before_in_document_order(
            h,
            NodeId::new(3),
            NodeId::new(1)
        ));
    }

    #[test]
    fn is_before_in_document_order_is_antisymmetric_across_a_nested_tree() {
        let sd = nested_body();
        let h = &sd.node_hierarchy;
        for a in 0..3 {
            for b in 0..3 {
                let ab = is_before_in_document_order(h, NodeId::new(a), NodeId::new(b));
                let ba = is_before_in_document_order(h, NodeId::new(b), NodeId::new(a));
                if a == b {
                    assert!(!ab && !ba, "a node is never before itself");
                } else {
                    assert_ne!(ab, ba, "exactly one of ({a},{b}) / ({b},{a}) must hold");
                }
            }
        }
    }

    #[test]
    fn is_before_in_document_order_is_deterministic_for_unknown_nodes() {
        let sd = flat_body(1);
        let h = &sd.node_hierarchy;
        // Out-of-range ids fall back to a single-element path; the comparison must
        // still terminate and return a stable answer instead of panicking.
        assert!(is_before_in_document_order(
            h,
            NodeId::ZERO,
            NodeId::new(usize::MAX)
        ));
        assert!(!is_before_in_document_order(
            h,
            NodeId::new(usize::MAX),
            NodeId::ZERO
        ));
    }

    #[test]
    fn collect_nodes_in_document_order_start_equals_end() {
        let sd = flat_body(2);
        assert_eq!(
            collect_nodes_in_document_order(&sd.node_hierarchy, NodeId::new(2), NodeId::new(2)),
            vec![NodeId::new(2)]
        );
        // Even a bogus id short-circuits to itself (documented start == end path).
        assert_eq!(
            collect_nodes_in_document_order(
                &sd.node_hierarchy,
                NodeId::new(usize::MAX),
                NodeId::new(usize::MAX)
            ),
            vec![NodeId::new(usize::MAX)]
        );
    }

    #[test]
    fn collect_nodes_in_document_order_walks_the_tree_in_pre_order() {
        let sd = flat_body(3); // body(0) > [1, 2, 3]
        assert_eq!(
            collect_nodes_in_document_order(&sd.node_hierarchy, NodeId::ZERO, NodeId::new(3)),
            vec![NodeId::ZERO, NodeId::new(1), NodeId::new(2), NodeId::new(3)]
        );
        assert_eq!(
            collect_nodes_in_document_order(&sd.node_hierarchy, NodeId::new(1), NodeId::new(2)),
            vec![NodeId::new(1), NodeId::new(2)]
        );

        // Nested: body(0) > div(1) > div(2) — pre-order is 0, 1, 2.
        let nested = nested_body();
        assert_eq!(
            collect_nodes_in_document_order(&nested.node_hierarchy, NodeId::ZERO, NodeId::new(2)),
            vec![NodeId::ZERO, NodeId::new(1), NodeId::new(2)]
        );
    }

    #[test]
    fn collect_nodes_in_document_order_terminates_when_end_precedes_start() {
        // The traversal hits `end` before it ever enters the range, so it bails
        // out with an empty result rather than looping forever.
        let sd = flat_body(3);
        let out =
            collect_nodes_in_document_order(&sd.node_hierarchy, NodeId::new(2), NodeId::new(1));
        assert!(out.is_empty());
    }

    #[test]
    fn collect_nodes_in_document_order_with_an_unreachable_end_stops_at_the_tree_end() {
        let sd = flat_body(3);
        let out = collect_nodes_in_document_order(
            &sd.node_hierarchy,
            NodeId::new(1),
            NodeId::new(usize::MAX),
        );
        assert_eq!(
            out,
            vec![NodeId::new(1), NodeId::new(2), NodeId::new(3)],
            "an end node that is never reached must terminate at the end of the traversal"
        );
    }

    // ---------------------------------------------------------------------
    // is_layout_equivalent
    // ---------------------------------------------------------------------

    #[test]
    fn is_layout_equivalent_holds_for_independently_built_identical_doms() {
        assert!(is_layout_equivalent(&flat_body(3), &flat_body(3)));
        assert!(is_layout_equivalent(
            &StyledDom::default(),
            &StyledDom::default()
        ));
        assert!(is_layout_equivalent(&nested_body(), &nested_body()));
    }

    #[test]
    fn is_layout_equivalent_rejects_a_different_node_count() {
        assert!(!is_layout_equivalent(&flat_body(3), &flat_body(4)));
        assert!(!is_layout_equivalent(&flat_body(0), &flat_body(1)));
    }

    #[test]
    fn is_layout_equivalent_rejects_a_different_structure() {
        // Same node count (3), different shape: [body > div > div] vs [body > div, div]
        assert!(!is_layout_equivalent(&nested_body(), &flat_body(2)));
    }

    #[test]
    fn is_layout_equivalent_rejects_a_changed_class() {
        let build = |class: &str| {
            let mut dom = Dom::create_body()
                .with_children(vec![Dom::create_div().with_class(class.to_string().into())].into());
            StyledDom::create(&mut dom, Css::empty())
        };
        assert!(is_layout_equivalent(&build("a"), &build("a")));
        assert!(!is_layout_equivalent(&build("a"), &build("b")));
    }

    #[test]
    fn is_layout_equivalent_rejects_a_changed_pseudo_state() {
        let base = flat_body(2);
        let mut hovered = flat_body(2);
        let _ = hovered.restyle_nodes_hover(&[NodeId::new(1)], true);
        assert!(
            !is_layout_equivalent(&base, &hovered),
            ":hover changes CSS resolution, so the DOMs are not layout-equivalent"
        );
    }

    // ---------------------------------------------------------------------
    // CompactDom + convert_dom_into_compact_dom
    // ---------------------------------------------------------------------

    #[test]
    fn compact_dom_len_and_is_empty() {
        let single = convert_dom_into_compact_dom(Dom::create_div());
        assert_eq!(single.len(), 1);
        assert!(!single.is_empty());

        let tree = convert_dom_into_compact_dom(
            Dom::create_body().with_children(vec![Dom::create_div(); 4].into()),
        );
        assert_eq!(tree.len(), 5);
        assert!(!tree.is_empty());

        // A hand-built zero-node arena is the only way to observe is_empty() == true.
        let empty = CompactDom {
            node_hierarchy: NodeHierarchy {
                internal: Vec::new(),
            },
            node_data: NodeDataContainer {
                internal: Vec::new(),
            },
            root: NodeId::ZERO,
        };
        assert_eq!(empty.len(), 0);
        assert!(empty.is_empty());
    }

    #[test]
    fn convert_dom_into_compact_dom_links_flat_siblings() {
        let compact = convert_dom_into_compact_dom(
            Dom::create_body().with_children(vec![Dom::create_div(); 3].into()),
        );
        assert_eq!(compact.len(), 4);
        assert_eq!(compact.root, NodeId::ZERO);

        let h = compact.node_hierarchy.as_ref();
        assert_eq!(h[NodeId::ZERO].parent, None);
        assert_eq!(h[NodeId::ZERO].last_child, Some(NodeId::new(3)));

        for i in 1..=3usize {
            assert_eq!(h[NodeId::new(i)].parent, Some(NodeId::ZERO));
            let expected_next = if i == 3 {
                None
            } else {
                Some(NodeId::new(i + 1))
            };
            assert_eq!(h[NodeId::new(i)].next_sibling, expected_next);
            let expected_prev = if i == 1 {
                None
            } else {
                Some(NodeId::new(i - 1))
            };
            assert_eq!(h[NodeId::new(i)].previous_sibling, expected_prev);
            assert_eq!(
                h[NodeId::new(i)].last_child,
                None,
                "the children are leaves"
            );
        }
    }

    /// ADVERSARIAL: `last_child` must name the last DIRECT child — that is the
    /// contract `NodeHierarchyItem::last_child_id()` documents, the one
    /// `az_reverse_children` walks backwards from, and the one `append_child`
    /// splices new siblings onto. The flat encoding computes it as
    /// `node_id + estimated_total_children`, which is the last node of the whole
    /// SUBTREE — those coincide only when the last direct child is a leaf.
    #[test]
    fn convert_dom_into_compact_dom_last_child_is_the_last_direct_child() {
        // body(0) > div(1) > div(2): the body's only direct child is node 1.
        let sd = nested_body();
        let h = sd.node_hierarchy.as_container();

        let last_direct_child = NodeId::ZERO.az_children(&h).last();
        assert_eq!(last_direct_child, Some(NodeId::new(1)));
        assert_eq!(
            h[NodeId::ZERO].last_child_id(),
            last_direct_child,
            "last_child_id() must agree with the forward child iteration"
        );
    }

    #[test]
    fn convert_dom_into_compact_dom_handles_an_empty_and_a_deep_tree() {
        assert_eq!(convert_dom_into_compact_dom(Dom::create_body()).len(), 1);

        let mut deep = Dom::create_div();
        for _ in 0..64 {
            deep = Dom::create_div().with_children(vec![deep].into());
        }
        let compact = convert_dom_into_compact_dom(deep);
        assert_eq!(compact.len(), 65);
        // Pre-order ids: every node's parent is the node right before it.
        let h = compact.node_hierarchy.as_ref();
        for i in 1..65usize {
            assert_eq!(h[NodeId::new(i)].parent, Some(NodeId::new(i - 1)));
        }
    }

    // ---------------------------------------------------------------------
    // scope_inline_css / collect_css_from_dom / strip_css_from_dom
    // ---------------------------------------------------------------------

    #[test]
    fn scope_inline_css_advances_next_id_once_per_node() {
        let mut dom = Dom::create_body().with_children(
            vec![
                Dom::create_div().with_children(vec![Dom::create_div()].into()),
                Dom::create_div(),
            ]
            .into(),
        );
        let _ = dom.fixup_children_estimated();

        let mut next = 0usize;
        scope_inline_css(&mut dom, &mut next);
        assert_eq!(
            next, 4,
            "4 nodes → the counter must land on 4 (pre-order ids 0..3)"
        );
    }

    #[test]
    fn scope_inline_css_from_zero_and_from_a_large_offset() {
        let mut leaf = Dom::create_div();
        let _ = leaf.fixup_children_estimated();
        let mut next = 0usize;
        scope_inline_css(&mut leaf, &mut next);
        assert_eq!(next, 1, "a single leaf consumes exactly one id");

        // A large (but non-saturating) starting id must not panic or wrap.
        let mut dom = Dom::create_body().with_children(vec![Dom::create_div(); 2].into());
        let _ = dom.fixup_children_estimated();
        let mut big = 1_000_000usize;
        scope_inline_css(&mut dom, &mut big);
        assert_eq!(big, 1_000_003);
    }

    #[test]
    fn scope_inline_css_preserves_the_rule_count_of_every_node() {
        let mut dom = Dom::create_body()
            .with_css("color: red")
            .with_children(vec![Dom::create_div().with_css("width: 5px")].into());
        let _ = dom.fixup_children_estimated();

        let rules_before: usize = dom
            .css
            .as_ref()
            .iter()
            .map(|c| c.rules.as_ref().len())
            .sum::<usize>()
            + dom.children.as_ref()[0]
                .css
                .as_ref()
                .iter()
                .map(|c| c.rules.as_ref().len())
                .sum::<usize>();
        assert!(rules_before > 0, "with_css must produce at least one rule");

        let mut next = 0usize;
        scope_inline_css(&mut dom, &mut next);

        let rules_after: usize = dom
            .css
            .as_ref()
            .iter()
            .map(|c| c.rules.as_ref().len())
            .sum::<usize>()
            + dom.children.as_ref()[0]
                .css
                .as_ref()
                .iter()
                .map(|c| c.rules.as_ref().len())
                .sum::<usize>();
        assert_eq!(
            rules_before, rules_after,
            "scoping rewrites paths in place; it must not add or drop rules"
        );
        assert_eq!(next, 2);
    }

    #[test]
    fn collect_css_from_dom_yields_inner_css_before_outer_css() {
        let outer = parse_css("div { color: red; } span { color: blue; }");
        let inner = parse_css("p { color: green; }");
        let outer_rules = outer.rules.as_ref().len();
        let inner_rules = inner.rules.as_ref().len();
        assert_ne!(
            outer_rules, inner_rules,
            "the two stylesheets must be distinguishable by rule count"
        );

        let mut child = Dom::create_div();
        child.add_component_css(inner);
        let mut dom = Dom::create_body().with_children(vec![child].into());
        dom.add_component_css(outer);

        let mut out = Vec::new();
        collect_css_from_dom(&dom, &mut out);

        assert_eq!(out.len(), 2);
        assert_eq!(
            out[0].rules.as_ref().len(),
            inner_rules,
            "deeper CSS is collected first (lower cascade priority)"
        );
        assert_eq!(out[1].rules.as_ref().len(), outer_rules);
    }

    #[test]
    fn collect_css_from_dom_on_a_css_free_tree_appends_nothing() {
        let dom = Dom::create_body().with_children(vec![Dom::create_div(); 3].into());
        let mut out = Vec::new();
        collect_css_from_dom(&dom, &mut out);
        assert!(out.is_empty());

        // ...and an already-populated `out` is appended to, not replaced.
        let mut prefilled = vec![Css::empty()];
        collect_css_from_dom(&dom, &mut prefilled);
        assert_eq!(prefilled.len(), 1);
    }

    #[test]
    fn strip_css_from_dom_clears_every_node_recursively() {
        let mut dom = Dom::create_body().with_css("color: red").with_children(
            vec![Dom::create_div()
                .with_css("width: 5px")
                .with_children(vec![Dom::create_div().with_css("height: 5px")].into())]
            .into(),
        );
        assert!(!dom.css.as_ref().is_empty());

        strip_css_from_dom(&mut dom);

        assert!(dom.css.as_ref().is_empty());
        let child = &dom.children.as_ref()[0];
        assert!(child.css.as_ref().is_empty());
        assert!(child.children.as_ref()[0].css.as_ref().is_empty());

        // Idempotent.
        strip_css_from_dom(&mut dom);
        assert!(dom.css.as_ref().is_empty());
    }

    // ---------------------------------------------------------------------
    // hierarchy_ancestors — THE DOM ancestor walk, explicit inclusivity
    // ---------------------------------------------------------------------

    /// 0 <- 1 <- 2 (the `parent` field is 1-based encoded: 0 = none).
    fn linear_hierarchy() -> [NodeHierarchyItem; 3] {
        [
            NodeHierarchyItem {
                parent: 0,
                previous_sibling: 0,
                next_sibling: 0,
                last_child: 2,
            },
            NodeHierarchyItem {
                parent: 1,
                previous_sibling: 0,
                next_sibling: 0,
                last_child: 3,
            },
            NodeHierarchyItem {
                parent: 2,
                previous_sibling: 0,
                next_sibling: 0,
                last_child: 0,
            },
        ]
    }

    fn walk(h: &[NodeHierarchyItem], n: usize, incl: crate::spaces::Inclusivity) -> Vec<usize> {
        hierarchy_ancestors(h, NodeId::new(n), incl)
            .map(|n| n.index())
            .collect()
    }

    #[test]
    fn hierarchy_ancestors_inclusivity_decides_only_the_first_element() {
        use crate::spaces::Inclusivity;
        let h = linear_hierarchy();
        assert_eq!(walk(&h, 2, Inclusivity::SelfAndAncestors), vec![2, 1, 0]);
        assert_eq!(walk(&h, 2, Inclusivity::AncestorsOnly), vec![1, 0]);
        assert_eq!(walk(&h, 0, Inclusivity::SelfAndAncestors), vec![0]);
        assert!(walk(&h, 0, Inclusivity::AncestorsOnly).is_empty());
    }

    #[test]
    fn hierarchy_ancestors_survives_empty_and_out_of_range_input() {
        use crate::spaces::Inclusivity;
        for incl in [Inclusivity::AncestorsOnly, Inclusivity::SelfAndAncestors] {
            // An empty hierarchy has a zero budget, so nothing is yielded even
            // self-inclusively — and, crucially, nothing is indexed.
            assert!(walk(&[], 0, incl).is_empty());
            assert!(walk(&[], 9_999, incl).is_empty());
        }
        let h = linear_hierarchy();
        assert!(walk(&h, 9_999, Inclusivity::AncestorsOnly).is_empty());
        assert_eq!(walk(&h, 9_999, Inclusivity::SelfAndAncestors), vec![9_999]);
    }

    #[test]
    fn hierarchy_ancestors_terminates_on_a_cycle() {
        use crate::spaces::Inclusivity;
        // 0 -> 1 -> 0
        let h = [
            NodeHierarchyItem {
                parent: 2,
                previous_sibling: 0,
                next_sibling: 0,
                last_child: 0,
            },
            NodeHierarchyItem {
                parent: 1,
                previous_sibling: 0,
                next_sibling: 0,
                last_child: 0,
            },
        ];
        let walked = walk(&h, 0, Inclusivity::SelfAndAncestors);
        assert!(
            walked.len() <= h.len(),
            "the budget must stop a cyclic chain"
        );
    }
}
