#[allow(unused_imports)]
pub use super::*;
#[cfg(test)]
mod audit_tests {
    use azul_css::props::basic::StyleFontFamily;

    use super::*;

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
            placeholder: all,
            seat_focused: all,
            focus_visible: all,
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
        // Each setter must light up exactly one state index in 1..=12.
        let setters: [(u8, fn(&mut StyledNodeState)); 12] = [
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
            (11, |s| s.seat_focused = true),
            (12, |s| s.focus_visible = true),
        ];

        for (expected_idx, set) in setters {
            let mut s = StyledNodeState::new();
            set(&mut s);
            assert!(!s.is_normal(), "state {expected_idx} must not be 'normal'");
            for idx in 1..=12u8 {
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
        for idx in 13..=u8::MAX {
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
        for idx in 0..=11u8 {
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
    fn subtree_len_of_a_last_child_ends_where_an_ancestor_has_a_next_sibling() {
        // body(0) > [ row(1) > [ label(2), track(3) > knob(4) ], row(5) > track(6) ]
        // The first row's track is its LAST child: no next sibling of its own,
        // and yet the second row (5, 6) is not below it. The callers read
        // `index + 1 .. index + 1 + subtree_len` as the node's descendants -
        // a colour fade's patch range, a form's fields, a scoped stylesheet.
        let row = |children: Vec<Dom>| Dom::create_div().with_children(children.into());
        let mut dom = Dom::create_body().with_children(
            vec![
                row(vec![
                    Dom::create_div(),
                    Dom::create_div().with_children(vec![Dom::create_div()].into()),
                ]),
                row(vec![Dom::create_div()]),
            ]
            .into(),
        );
        let sd = StyledDom::create(&mut dom, Css::empty());
        let h = sd.node_hierarchy.as_container();
        assert_eq!(h.len(), 7, "harness: seven nodes in pre-order");
        // (node, descendants): the first track holds its knob only, the knob
        // is a leaf, the first row holds label + track + knob, the second
        // row its track, the very last node nothing, the root everything.
        for (node, descendants) in [(3, 1), (4, 0), (1, 3), (5, 1), (6, 0), (0, 6)] {
            assert_eq!(
                h.subtree_len(NodeId::new(node)),
                descendants,
                "node {node} has {descendants} descendants"
            );
        }
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

    /// miniword ENGINE-ISSUE 4:
    /// `Dom::create_text_do_not_use_without_block_level_wrapper(..).with_css(..)` silently
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
        use azul_css::props::{layout::display::LayoutDisplay, property::CssProperty};

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

    /// A node's `Dom::with_css` sheet is cascaded from the collected author CSS, not kept in
    /// `NodeData::style`, so two builds that differ only in it looked equivalent: a rebuild
    /// that only changed such a value (AzCalendar's zoom: every hour `height: 72px` instead of
    /// `48px`) kept the previous layout, and the change never showed.
    #[test]
    fn is_layout_equivalent_rejects_a_changed_node_stylesheet() {
        let build = |height: &str| {
            let mut dom = Dom::create_body().with_children(
                vec![Dom::create_div().with_css(&format!("height: {height};"))].into(),
            );
            StyledDom::create(&mut dom, Css::empty())
        };
        assert!(is_layout_equivalent(&build("48px"), &build("48px")));
        assert!(
            !is_layout_equivalent(&build("48px"), &build("72px")),
            "a node whose with_css height changed must not reuse the old layout"
        );
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

/// Theme-chain analysis 2026-09-12, item 1: a theme flip is a RESTYLE.
///
/// `set_dynamic_selector_context` used to rebuild the compact cache only, so
/// `cascaded_props` / `computed_values` — what the slow path and the display
/// list read — kept the answers of the context the DOM was created under.
/// These pin the re-cascade and its idempotence (a UA entry is replaced, never
/// duplicated, on every flip).
#[cfg(test)]
mod theme_flip_is_a_restyle {
    use azul_css::{
        dynamic_selector::{DynamicSelectorContext, PseudoStateType, ThemeCondition},
        props::property::CssPropertyType,
    };

    use super::*;
    use crate::dom::NodeType;

    fn ctx(theme: azul_css::system::DarkLightMode) -> DynamicSelectorContext {
        DynamicSelectorContext {
            mode: theme,
            ..Default::default()
        }
    }

    /// `body > button`, no author css, no inline style: the button's border
    /// colour is a themed UA default (light `#c8c8c8`, dark `#5a5a5a`).
    fn body_with_button() -> Dom {
        Dom::create_body().with_child(Dom::create_node(NodeType::Button))
    }

    const BUTTON: NodeId = NodeId::new(1);

    fn border_top_color(sd: &StyledDom) -> Option<CssProperty> {
        let node_data = sd.node_data.as_container();
        sd.get_css_property_cache()
            .get_property_slow(
                &node_data[BUTTON],
                &BUTTON,
                &StyledNodeState::default(),
                &CssPropertyType::BorderTopColor,
            )
            .cloned()
    }

    fn ua_border_top_color(theme: azul_css::system::DarkLightMode) -> CssProperty {
        crate::ua_css::get_ua_property_themed(
            &NodeType::Button,
            CssPropertyType::BorderTopColor,
            Some(&ctx(theme)),
        )
        .cloned()
        .expect("the UA sheet defines the button border")
    }

    /// The Normal-state `BorderTopColor` entries in the button's cascaded
    /// slice, with their origin tag.
    fn cascaded_border_entries(sd: &StyledDom) -> Vec<(CssProperty, bool)> {
        sd.get_css_property_cache()
            .cascaded_props
            .get_slice(BUTTON.index())
            .iter()
            .filter(|p| {
                p.state == PseudoStateType::Normal && p.prop_type == CssPropertyType::BorderTopColor
            })
            .map(|p| (p.property.clone(), p.ua_origin))
            .collect()
    }

    #[test]
    fn a_theme_flip_replaces_the_ua_defaults_in_cascaded_props() {
        let mut dom = body_with_button();
        let mut sd = StyledDom::create(&mut dom, Css::empty());
        assert_eq!(
            border_top_color(&sd),
            Some(ua_border_top_color(azul_css::system::DarkLightMode::Light)),
            "no context yet: the light table"
        );

        sd.set_dynamic_selector_context(ctx(azul_css::system::DarkLightMode::Dark));
        assert_eq!(
            border_top_color(&sd),
            Some(ua_border_top_color(azul_css::system::DarkLightMode::Dark)),
            "the dark window's border must be the dark twin, in the resolved style"
        );
        assert_eq!(
            cascaded_border_entries(&sd),
            vec![(ua_border_top_color(azul_css::system::DarkLightMode::Dark), true)],
            "exactly ONE UA-origin entry: the light one was stripped, not shadowed"
        );

        sd.set_dynamic_selector_context(ctx(azul_css::system::DarkLightMode::Light));
        assert_eq!(
            border_top_color(&sd),
            Some(ua_border_top_color(azul_css::system::DarkLightMode::Light)),
            "and back"
        );
        assert_eq!(
            cascaded_border_entries(&sd),
            vec![(ua_border_top_color(azul_css::system::DarkLightMode::Light), true)],
            "still one entry after the second flip"
        );
    }

    #[test]
    fn a_dom_created_under_a_context_is_cascaded_for_it_at_once() {
        let mut dom = body_with_button();
        let mut sd =
            StyledDom::create_with_context(&mut dom, Css::empty(), Some(ctx(azul_css::system::DarkLightMode::Dark)));
        assert_eq!(
            border_top_color(&sd),
            Some(ua_border_top_color(azul_css::system::DarkLightMode::Dark)),
            "born dark: the first cascade already answered the dark table"
        );
        // (Creation prunes the compact-encoded Normal entries out of
        // `cascaded_props` — the compact cache and `computed_values` carry
        // them — so the border is asserted through the readers, not the
        // store: the slow path above, the compact tier here.)
        let dark_raw = sd
            .get_css_property_cache()
            .compact_cache
            .as_ref()
            .map(|cc| cc.get_border_top_color_raw(BUTTON.index()))
            .expect("compact cache built at creation");
        assert_ne!(dark_raw, 0, "the border is in the compact tier");
        let epoch_before = sd.get_css_property_cache().cascade_epoch;
        let cascaded_before = sd.get_css_property_cache().cascaded_props.clone();

        // The funnel's offer of the SAME context is a no-op: no new
        // generation, no second cascade, nothing pushed twice.
        sd.set_dynamic_selector_context(ctx(azul_css::system::DarkLightMode::Dark));
        assert_eq!(
            sd.get_css_property_cache().cascade_epoch,
            epoch_before,
            "same context: not a new generation"
        );
        assert_eq!(sd.get_css_property_cache().cascaded_props, cascaded_before);
        assert_eq!(
            border_top_color(&sd),
            Some(ua_border_top_color(azul_css::system::DarkLightMode::Dark))
        );
    }

    #[test]
    fn create_from_dom_with_context_threads_the_context_through() {
        let sd = StyledDom::create_from_dom_with_context(
            body_with_button(),
            Some(ctx(azul_css::system::DarkLightMode::Dark)),
        );
        assert_eq!(
            sd.get_css_property_cache()
                .dynamic_context
                .as_deref()
                .map(|c| c.mode),
            Some(azul_css::system::DarkLightMode::Dark)
        );
        assert_eq!(
            border_top_color(&sd),
            Some(ua_border_top_color(azul_css::system::DarkLightMode::Dark))
        );
    }

    /// `restyle` with an empty sheet used to push into a flattened (emptied)
    /// build vec and panic; a theme flip on an inline-styled widget DOM is
    /// exactly that call.
    #[test]
    fn restyle_with_an_empty_stylesheet_recascades_instead_of_panicking() {
        let mut dom = body_with_button();
        let mut sd = StyledDom::create(&mut dom, Css::empty());
        sd.restyle(Css::empty());
        assert_eq!(
            border_top_color(&sd),
            Some(ua_border_top_color(azul_css::system::DarkLightMode::Light))
        );
        assert_eq!(
            cascaded_border_entries(&sd).len(),
            1,
            "a second full cascade must not duplicate the UA entry"
        );
        assert!(sd.get_css_property_cache().ua_applied);
    }

    #[test]
    fn a_context_change_that_keeps_the_theme_costs_nothing_on_a_plain_dom() {
        let mut dom = body_with_button();
        let mut sd = StyledDom::create_with_context(
            &mut dom,
            Css::empty(),
            Some(ctx(azul_css::system::DarkLightMode::Light)),
        );
        let before = sd.get_css_property_cache().cascaded_props.clone();
        // A resize: same theme, different viewport.
        let resized = ctx(azul_css::system::DarkLightMode::Light).with_viewport(320.0, 240.0);
        sd.set_dynamic_selector_context(resized);
        assert_eq!(
            sd.get_css_property_cache().cascaded_props,
            before,
            "no conditional declaration anywhere, same theme: the cascade must be untouched"
        );
    }
    // ------------------------------------------------------------------
    // Item 3: one themed UA table, the text colour IN the resolved style
    // ------------------------------------------------------------------

    #[test]
    fn a_theme_flip_rebuilds_the_compact_cache_from_the_new_defaults() {
        let mut dom = body_with_button();
        let mut sd = StyledDom::create(&mut dom, Css::empty());
        let light_raw = sd
            .get_css_property_cache()
            .compact_cache
            .as_ref()
            .map(|cc| cc.get_border_top_color_raw(BUTTON.index()))
            .expect("compact cache built at creation");

        sd.set_dynamic_selector_context(ctx(azul_css::system::DarkLightMode::Dark));
        let dark_raw = sd
            .get_css_property_cache()
            .compact_cache
            .as_ref()
            .map(|cc| cc.get_border_top_color_raw(BUTTON.index()))
            .expect("compact cache rebuilt on the flip");
        assert_ne!(light_raw, 0, "the light border is in the compact tier");
        assert_ne!(dark_raw, 0, "the dark border is in the compact tier");
        assert_ne!(
            light_raw, dark_raw,
            "the compact (normal-state fast path) tier must follow the theme too"
        );
    }

    /// `body > p > "text"`, nothing styled: the text colour is the UA's
    /// document default, and it must be IN the resolved style of every node
    /// on every reader — root `cascaded_props`, descendant `computed_values`
    /// (the slow path), the compact text tier (the fast path).
    fn body_p_text() -> Dom {
        Dom::create_body().with_child(
            Dom::create_p()
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("5")),
        )
    }

    const ROOT: NodeId = NodeId::new(0);
    const TEXT: NodeId = NodeId::new(2);

    fn slow_text_color(sd: &StyledDom, node: NodeId) -> Option<(u8, u8, u8)> {
        let node_data = sd.node_data.as_container();
        match sd.get_css_property_cache().get_property_slow(
            &node_data[node],
            &node,
            &StyledNodeState::default(),
            &CssPropertyType::TextColor,
        ) {
            Some(CssProperty::TextColor(v)) => {
                v.get_property().map(|c| (c.inner.r, c.inner.g, c.inner.b))
            }
            _ => None,
        }
    }

    fn compact_text_color(sd: &StyledDom, node: NodeId) -> Option<(u8, u8, u8)> {
        let raw = sd
            .get_css_property_cache()
            .compact_cache
            .as_ref()
            .map(|cc| cc.get_text_color_raw(node.index()))
            .unwrap_or(0);
        (raw != 0).then_some(((raw >> 24) as u8, (raw >> 16) as u8, (raw >> 8) as u8))
    }

    const BLACK: (u8, u8, u8) = (0, 0, 0);
    const DARK_INK: (u8, u8, u8) = (0xe8, 0xe8, 0xe8);

    #[test]
    fn the_ua_text_colour_is_cascaded_onto_the_root_and_inherited_below() {
        let mut dom = body_p_text();
        let sd =
            StyledDom::create_with_context(&mut dom, Css::empty(), Some(ctx(azul_css::system::DarkLightMode::Dark)));
        // `computed_values` is the record of the cascade (creation prunes
        // the compact-encoded Normal entries out of `cascaded_props` once
        // the compact cache holds them): the root OWNS the colour — it came
        // from the UA table, not from a parent — and every descendant
        // INHERITS it rather than declaring one of its own (a UA `color` on
        // a child would block inheritance of an author colour above it).
        use crate::prop_cache::CssPropertyOrigin;
        let cv = &sd.get_css_property_cache().computed_values;
        let root = cv
            .get(ROOT.index(), CssPropertyType::TextColor)
            .expect("the root's resolved style carries `color`");
        assert_eq!(
            root.origin,
            CssPropertyOrigin::Own,
            "cascaded ONTO the root"
        );
        for node in [NodeId::new(1), TEXT] {
            let v = cv
                .get(node.index(), CssPropertyType::TextColor)
                .unwrap_or_else(|| panic!("node {} has no resolved `color`", node.index()));
            assert_eq!(
                v.origin,
                CssPropertyOrigin::Inherited,
                "node {} must inherit the root's colour, not own a UA one",
                node.index()
            );
        }
        assert_eq!(slow_text_color(&sd, ROOT), Some(DARK_INK));
        assert_eq!(
            slow_text_color(&sd, TEXT),
            Some(DARK_INK),
            "computed_values"
        );
        assert_eq!(
            compact_text_color(&sd, TEXT),
            Some(DARK_INK),
            "compact text tier"
        );
    }

    #[test]
    fn the_two_readers_agree_on_the_text_colour_under_both_themes_and_across_a_flip() {
        let mut dom = body_p_text();
        let mut sd = StyledDom::create(&mut dom, Css::empty());
        assert_eq!(slow_text_color(&sd, TEXT), Some(BLACK), "no context: light");
        assert_eq!(compact_text_color(&sd, TEXT), Some(BLACK));

        sd.set_dynamic_selector_context(ctx(azul_css::system::DarkLightMode::Dark));
        assert_eq!(
            slow_text_color(&sd, TEXT),
            Some(DARK_INK),
            "dark: slow path"
        );
        assert_eq!(
            compact_text_color(&sd, TEXT),
            Some(DARK_INK),
            "dark: fast path"
        );
        // The re-cascade does not prune, so here the store shows the entry
        // itself: exactly one, UA-origin, on the root and nowhere else.
        let ua_color_entries = |node: NodeId| -> Vec<bool> {
            sd.get_css_property_cache()
                .cascaded_props
                .get_slice(node.index())
                .iter()
                .filter(|p| p.prop_type == CssPropertyType::TextColor)
                .map(|p| p.ua_origin)
                .collect()
        };
        assert_eq!(
            ua_color_entries(ROOT),
            vec![true],
            "one UA `color` on the root"
        );
        assert_eq!(ua_color_entries(NodeId::new(1)), Vec::<bool>::new());
        assert_eq!(ua_color_entries(TEXT), Vec::<bool>::new());

        sd.set_dynamic_selector_context(ctx(azul_css::system::DarkLightMode::Light));
        assert_eq!(slow_text_color(&sd, TEXT), Some(BLACK), "back: slow path");
        assert_eq!(
            compact_text_color(&sd, TEXT),
            Some(BLACK),
            "back: fast path"
        );
    }

    /// A UA default an ELEMENT gets - a link's `color` under a wrapper that
    /// declares one - is inherited by the text inside it, and owned by nobody
    /// below. The inheritance walk runs before the UA pass and had copied the
    /// wrapper's colour onto the text; that stale copy is REMOVED, not
    /// rewritten into a UA entry of the text's own: a child never owns a UA
    /// colour (one that did would block an author colour above it).
    #[test]
    fn the_text_of_a_link_inherits_the_links_ua_colour_and_owns_none() {
        use crate::{
            dom::{AttributeType, NodeType},
            prop_cache::CssPropertyOrigin,
        };
        let mut wrapper = Dom::create_div();
        wrapper.root.set_css("color: rgb(51, 51, 51);");
        let link = Dom::create_node(NodeType::A)
            .with_attribute(AttributeType::Href("https://a.b/".into()))
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                "link",
            ));
        let mut dom = Dom::create_body().with_child(wrapper.with_child(link));
        let sd = StyledDom::create(&mut dom, Css::empty());
        // body(0) > div(1) > a(2) > "link"(3)
        let (link, text) = (NodeId::new(2), NodeId::new(3));
        const LINK_BLUE: (u8, u8, u8) = (0x00, 0x00, 0xee);
        assert_eq!(
            slow_text_color(&sd, link),
            Some(LINK_BLUE),
            "the link: its UA colour beats the wrapper's"
        );
        assert_eq!(
            slow_text_color(&sd, text),
            Some(LINK_BLUE),
            "the text: slow path"
        );
        assert_eq!(
            compact_text_color(&sd, text),
            Some(LINK_BLUE),
            "the text: compact tier"
        );
        let cv = &sd.get_css_property_cache().computed_values;
        let own = cv
            .get(link.index(), CssPropertyType::TextColor)
            .expect("the link resolves `color`");
        assert_eq!(
            own.origin,
            CssPropertyOrigin::Own,
            "the link owns its UA colour"
        );
        let inherited = cv
            .get(text.index(), CssPropertyType::TextColor)
            .expect("the text resolves `color`");
        assert_eq!(
            inherited.origin,
            CssPropertyOrigin::Inherited,
            "the text INHERITS it: the wrapper's stale copy is gone and no entry took its place"
        );
    }

    #[test]
    fn get_text_color_or_default_never_needs_its_default_on_a_cascaded_dom() {
        let mut dom = body_p_text();
        let sd =
            StyledDom::create_with_context(&mut dom, Css::empty(), Some(ctx(azul_css::system::DarkLightMode::Dark)));
        let node_data = sd.node_data.as_container();
        for i in 0..sd.node_count() {
            let n = NodeId::new(i);
            let c = sd.get_css_property_cache().get_text_color_or_default(
                &node_data[n],
                &n,
                &StyledNodeState::default(),
            );
            assert_eq!((c.inner.r, c.inner.g, c.inner.b), DARK_INK, "node {i}");
        }
    }

    #[test]
    fn an_author_colour_on_the_root_beats_the_ua_default() {
        let mut dom = body_p_text();
        dom.set_css("color: rgb(10, 20, 30);");
        let sd =
            StyledDom::create_with_context(&mut dom, Css::empty(), Some(ctx(azul_css::system::DarkLightMode::Dark)));
        assert_eq!(slow_text_color(&sd, TEXT), Some((10, 20, 30)));
        assert_eq!(compact_text_color(&sd, TEXT), Some((10, 20, 30)));
        assert!(
            sd.get_css_property_cache()
                .cascaded_props
                .get_slice(ROOT.index())
                .iter()
                .all(|p| !(p.prop_type == CssPropertyType::TextColor && p.ua_origin)),
            "an inline `color` on the root suppresses the UA entry"
        );
    }
}

/// Theme-chain analysis 2026-09-12, item 4: `CssPropertyCache::cascade_epoch`
/// is the ONE key that tells two cascades of one DOM apart for every cache
/// that serves painted output.
#[cfg(test)]
mod cascade_epoch {
    use azul_css::{
        dynamic_selector::{DynamicSelectorContext, ThemeCondition},
        props::{basic::color::ColorU, style::StyleTextColor},
    };

    use super::*;

    fn ctx(theme: azul_css::system::DarkLightMode) -> DynamicSelectorContext {
        DynamicSelectorContext {
            mode: theme,
            ..Default::default()
        }
    }

    fn fixture() -> StyledDom {
        let mut dom = Dom::create_body().with_child(
            Dom::create_p()
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("5")),
        );
        StyledDom::create_with_context(&mut dom, Css::empty(), Some(ctx(azul_css::system::DarkLightMode::Light)))
    }

    fn epoch(sd: &StyledDom) -> u64 {
        sd.get_css_property_cache().cascade_epoch
    }

    fn red() -> CssProperty {
        CssProperty::const_text_color(StyleTextColor {
            inner: ColorU::rgb(255, 0, 0),
        })
    }

    #[test]
    fn a_theme_flip_bumps_the_epoch_and_an_equal_offer_does_not() {
        let mut sd = fixture();
        let e0 = epoch(&sd);
        sd.set_dynamic_selector_context(ctx(azul_css::system::DarkLightMode::Light));
        assert_eq!(epoch(&sd), e0, "the same context again is free");
        sd.set_dynamic_selector_context(ctx(azul_css::system::DarkLightMode::Dark));
        assert_ne!(epoch(&sd), e0, "a flip is a new generation");
    }

    #[test]
    fn a_context_change_without_a_recascade_still_bumps() {
        // Plain DOM, same theme, different viewport: nothing re-cascades,
        // but the context is a cascade input and the DL key must move.
        let mut sd = fixture();
        let e0 = epoch(&sd);
        sd.set_dynamic_selector_context(ctx(azul_css::system::DarkLightMode::Light).with_viewport(1.0, 1.0));
        assert_ne!(epoch(&sd), e0);
    }

    #[test]
    fn a_restyle_bumps() {
        let mut sd = fixture();
        let e0 = epoch(&sd);
        sd.restyle(Css::empty());
        assert_ne!(epoch(&sd), e0);
    }

    #[test]
    fn a_user_override_bumps_but_the_per_tick_channel_does_not() {
        let mut sd = fixture();
        let e0 = epoch(&sd);
        let _ = sd.restyle_user_property(&NodeId::new(1), &[red()]);
        let e1 = epoch(&sd);
        assert_ne!(e1, e0, "restyle_user_property changes the resolved style");

        sd.set_user_property_override_fast(&NodeId::new(1), &[red()]);
        assert_eq!(
            epoch(&sd),
            e1,
            "the animation channel patches the display list itself and must not invalidate the DL \
             cache every tick"
        );
    }

    #[test]
    fn a_recompute_of_the_compact_cache_bumps() {
        let mut sd = fixture();
        let e0 = epoch(&sd);
        sd.recompute_inheritance_and_compact_cache();
        assert_ne!(epoch(&sd), e0);
    }

    /// Design §9.1 pitfall 11: a custom property is a cascade input like the
    /// theme. A context change that moves a variable is a new generation
    /// (every cache serving painted output keys on the epoch), and it moves
    /// exactly the nodes that read the variable.
    #[test]
    fn a_variable_that_follows_the_mode_bumps_the_epoch_and_moves_only_its_readers() {
        let mut dom = Dom::create_body()
            .with_child(
                Dom::create_div().with_style(Css::parse_inline("color: var(--fg, #ff0000)")),
            )
            .with_child(Dom::create_div().with_style(Css::parse_inline("color: #123456")));
        let css = Css::from_string(
            "@theme(dark) { :root { --fg: #ffffff; } } @theme(light) { :root { --fg: #000000; } }"
                .into(),
        );
        let mut sd =
            StyledDom::create_with_context(&mut dom, css, Some(ctx(azul_css::system::DarkLightMode::Light)));
        let text = |sd: &StyledDom, n: usize| {
            sd.get_css_property_cache()
                .compact_cache
                .as_ref()
                .expect("compact cache")
                .tier2b_text[n]
                .text_color
        };
        let (e0, reader0, bystander0) = (epoch(&sd), text(&sd, 1), text(&sd, 2));

        sd.set_dynamic_selector_context(ctx(azul_css::system::DarkLightMode::Dark));
        assert_ne!(epoch(&sd), e0, "a variable change is a new generation");
        assert_ne!(text(&sd, 1), reader0, "the reader follows the variable");
        assert_eq!(text(&sd, 2), bystander0, "a node that reads nothing keeps its value");
    }
}

/// A DOM HOSTED by a node of another one - a `VirtualView`'s content -
/// inherits from it (`CssPropertyCache::inherited_from_host`) in every
/// cascade stage: the compact cache the layout reads, the slow path the
/// paint-time readers ask, and the walk that hands a parent's values to a
/// text node.
#[cfg(test)]
mod hosted_dom_inheritance_tests {
    use azul_css::{
        css::CssPropertyValue,
        props::{
            basic::color::ColorU,
            property::{CssProperty, CssPropertyType},
            style::{StyleCursor, StyleTextColor, StyleUserSelect},
        },
    };

    use super::*;
    use crate::prop_cache::{CssPropertyOrigin, CssPropertyWithOrigin, InheritedFromHost};

    const HOST_INK: ColorU = ColorU {
        r: 0x12,
        g: 0x34,
        b: 0x56,
        a: 0xff,
    };

    /// What a host with `color: ink; user-select: none; cursor: default`
    /// hands down.
    fn host(ink: ColorU) -> InheritedFromHost {
        let mut values: InheritedFromHost = [
            CssProperty::TextColor(CssPropertyValue::Exact(StyleTextColor { inner: ink })),
            CssProperty::UserSelect(CssPropertyValue::Exact(StyleUserSelect::None)),
            CssProperty::Cursor(CssPropertyValue::Exact(StyleCursor::Default)),
        ]
        .into_iter()
        .map(|property| {
            (
                property.get_type(),
                CssPropertyWithOrigin {
                    property,
                    origin: CssPropertyOrigin::Inherited,
                },
            )
        })
        .collect();
        values.sort_by_key(|(prop_type, _)| *prop_type);
        values
    }

    /// `<p>` (node 0) holding a text node (node 1), `p_css` on the `<p>`.
    fn hosted(p_css: &str, inherited: InheritedFromHost) -> StyledDom {
        let mut p = Dom::create_p_with_text("hosted");
        if !p_css.is_empty() {
            p = p.with_css(p_css);
        }
        StyledDom::create_from_dom_inheriting(p, None, &[], inherited)
    }

    /// The colour the text layout reads (the compact cache).
    fn ink(sd: &StyledDom, node: usize) -> u32 {
        sd.get_css_property_cache()
            .compact_cache
            .as_ref()
            .expect("compact cache")
            .get_text_color_raw(node)
    }

    const fn packed(c: ColorU) -> u32 {
        u32::from_be_bytes([c.r, c.g, c.b, c.a])
    }

    /// The value the slow path answers (hit testing, selection, paint).
    fn slow(sd: &StyledDom, node: usize, prop_type: CssPropertyType) -> Option<CssProperty> {
        let id = NodeId::new(node);
        let node_data = &sd.node_data.as_container()[id];
        sd.get_css_property_cache()
            .get_property(node_data, &id, &StyledNodeState::default(), &prop_type)
            .cloned()
    }

    #[test]
    fn a_hosted_root_takes_its_hosts_colour_over_the_document_default() {
        let sd = hosted("", host(HOST_INK));
        assert_eq!(
            ink(&sd, 0),
            packed(HOST_INK),
            "the hosted root takes its host's colour, not the document root's UA colour"
        );
        assert_eq!(ink(&sd, 1), packed(HOST_INK), "and hands it to its text");
        assert_eq!(
            slow(&sd, 1, CssPropertyType::TextColor),
            Some(CssProperty::TextColor(CssPropertyValue::Exact(StyleTextColor {
                inner: HOST_INK
            }))),
            "the slow path agrees"
        );

        let alone = hosted("", Vec::new());
        assert_ne!(
            ink(&alone, 1),
            packed(HOST_INK),
            "harness: a document of its own takes the UA colour"
        );
    }

    #[test]
    fn a_hosted_text_node_takes_its_hosts_user_select_and_cursor() {
        let sd = hosted("", host(HOST_INK));
        assert_eq!(
            slow(&sd, 1, CssPropertyType::UserSelect),
            Some(CssProperty::UserSelect(CssPropertyValue::Exact(
                StyleUserSelect::None
            )))
        );
        assert_eq!(
            slow(&sd, 1, CssPropertyType::Cursor),
            Some(CssProperty::Cursor(CssPropertyValue::Exact(StyleCursor::Default))),
            "a text node's UA I-beam yields to its host's cursor, as it yields to a parent's"
        );
    }

    #[test]
    fn a_hosted_doms_own_declaration_beats_what_its_host_hands_down() {
        let sd = hosted("color: #00aa00;", host(HOST_INK));
        let green = ColorU {
            r: 0,
            g: 0xaa,
            b: 0,
            a: 0xff,
        };
        assert_eq!(ink(&sd, 0), packed(green));
        assert_eq!(ink(&sd, 1), packed(green));
    }

    #[test]
    fn re_seeding_a_hosted_dom_re_cascades_only_when_the_host_moved() {
        let mut sd = hosted("", host(HOST_INK));
        assert!(
            !sd.set_inherited_from_host(host(HOST_INK)),
            "the values it already inherits: nothing re-runs"
        );
        let moved = ColorU {
            r: 0x65,
            g: 0x43,
            b: 0x21,
            a: 0xff,
        };
        assert!(sd.set_inherited_from_host(host(moved)));
        assert_eq!(ink(&sd, 1), packed(moved), "the text follows the host's new colour");
        assert_eq!(
            slow(&sd, 1, CssPropertyType::TextColor),
            Some(CssProperty::TextColor(CssPropertyValue::Exact(StyleTextColor {
                inner: moved
            })))
        );
    }
}

#[cfg(test)]
mod part_of_parent_tests {
    use super::*;

    /// `<body> > <button> > [<part> > <part-of-part>, <child>]`: node 2 is
    /// drawn as part of the button, node 3 as part of node 2, node 4 is an
    /// ordinary child.
    fn button_with_parts() -> StyledDom {
        let part = Dom::create_div()
            .with_part_of_parent(true)
            .with_child(Dom::create_div().with_part_of_parent(true));
        let mut dom = Dom::create_body().with_child(
            Dom::create_div()
                .with_child(part)
                .with_child(Dom::create_div()),
        );
        StyledDom::create(&mut dom, Css::empty())
    }

    #[test]
    fn a_part_of_a_pressed_node_is_pressed_with_it_and_an_ordinary_child_is_not() {
        let mut sd = button_with_parts();
        let _ = sd.restyle_on_state_change(
            None,
            None,
            Some(ActiveChange {
                deactivated: Vec::new(),
                activated: vec![NodeId::new(1), NodeId::ZERO],
            }),
        );
        assert!(sd.get_styled_node_state(&NodeId::new(2)).active, "the part");
        assert!(sd.get_styled_node_state(&NodeId::new(3)).active, "the part's own part");
        assert!(!sd.get_styled_node_state(&NodeId::new(4)).active, "an ordinary child");

        let _ = sd.restyle_on_state_change(
            None,
            None,
            Some(ActiveChange {
                deactivated: vec![NodeId::new(1), NodeId::ZERO],
                activated: Vec::new(),
            }),
        );
        assert!(!sd.get_styled_node_state(&NodeId::new(2)).active);
        assert!(!sd.get_styled_node_state(&NodeId::new(3)).active);
    }

    #[test]
    fn a_part_of_a_hovered_node_is_hovered_with_it() {
        let mut sd = button_with_parts();
        let _ = sd.restyle_nodes_hover(&[NodeId::new(1)], true);
        assert!(sd.get_styled_node_state(&NodeId::new(2)).hover);
        assert!(sd.get_styled_node_state(&NodeId::new(3)).hover);
        assert!(!sd.get_styled_node_state(&NodeId::new(4)).hover);
    }

    #[test]
    fn a_part_reports_the_properties_its_host_state_changed() {
        use azul_css::props::{basic::ColorU, property::CssProperty};
        let pressed = azul_css::dynamic_selector::CssPropertyWithConditions::on_active(
            CssProperty::const_text_color(azul_css::props::style::StyleTextColor {
                inner: ColorU::rgb(1, 2, 3),
            }),
        );
        let mut dom = Dom::create_body().with_child(Dom::create_div().with_child(
            Dom::create_div()
                .with_part_of_parent(true)
                .with_css_props(
                    azul_css::dynamic_selector::CssPropertyWithConditionsVec::from_vec(vec![pressed]),
                ),
        ));
        let mut sd = StyledDom::create(&mut dom, Css::empty());
        let changed = sd.restyle_nodes_active(&[NodeId::new(1)], true);
        assert!(
            changed.contains_key(&NodeId::new(2)),
            "the part's pressed colour is a change the display list must see: {changed:?}"
        );
    }
}

/// Guards of the 2026-11-21 audit, area A3-core (`audit/A3-core.md`).
#[cfg(all(test, feature = "css_parser"))]
mod audit_a3_core_tests {
    use super::*;

    /// A3-core-09: the sheets nodes carry are moved out of the tree, not
    /// copied and then dropped: in the order the copy listed them, and no
    /// node keeps one.
    #[test]
    fn take_css_from_dom_moves_what_collect_listed_and_strips_the_tree() {
        let tree = || {
            Dom::create_body()
                .with_css("color: red")
                .with_css("margin: 1px")
                .with_children(
                    vec![
                        Dom::create_div().with_css("width: 5px").with_children(
                            vec![Dom::create_div().with_css("height: 5px")].into(),
                        ),
                        Dom::create_div(),
                        Dom::create_div().with_css("width: 7px"),
                    ]
                    .into(),
                )
        };
        let mut listed = Vec::new();
        collect_css_from_dom(&tree(), &mut listed);
        assert_eq!(listed.len(), 5, "harness: five sheets");

        let mut dom = tree();
        let mut taken = Vec::new();
        take_css_from_dom(&mut dom, &mut taken);
        assert_eq!(taken, listed, "the same sheets in the same order");

        let mut left = Vec::new();
        collect_css_from_dom(&dom, &mut left);
        assert!(left.is_empty(), "no node keeps a sheet");

        // An `out` that holds sheets is appended to.
        let mut more = vec![Css::empty()];
        take_css_from_dom(&mut tree(), &mut more);
        assert_eq!(more.len(), 6);
    }

    /// A3-core-11b: `create_from_dom` prepared its `Dom` (exact node counts,
    /// each node's sheet scoped to its subtree, the sheets taken out), then
    /// handed it to the constructor that takes a `Dom` with a sheet, which
    /// prepared it again: every walk of the tree twice. Each node is visited
    /// once per walk now, and the DOM is styled as before.
    #[test]
    fn a_dom_is_prepared_for_styling_once() {
        let steps_for = |labels: usize| {
            let mut root = Dom::create_body().with_css("color: red;");
            for i in 0..labels {
                root = root.with_child(
                    Dom::create_span_with_text("label").with_css(&format!("width: {}px;", 10 + i % 7)),
                );
            }
            let styled = StyledDom::create_from_dom(root);
            let steps = styled.get_css_property_cache().dom_prepare_steps.get();
            (styled, steps)
        };
        let (styled, few) = steps_for(100);
        let (_, many) = steps_for(400);
        // body + 100 x (span + text) = 201 nodes, two walks (scope, take).
        assert_eq!(few, 2 * 201, "each node once per walk");
        assert_eq!(many, 2 * 801);
        // A label's own sheet still reaches its node and no other.
        let width = |node: usize| {
            let node_id = NodeId::new(node);
            styled
                .get_css_property_cache()
                .get_width(
                    &styled.node_data.as_container()[node_id],
                    &node_id,
                    &StyledNodeState::new(),
                )
                .cloned()
        };
        assert!(width(1).is_some(), "the first label has its width");
        assert_ne!(width(1), width(3), "10px and 11px");
        assert!(width(0).is_none(), "the body has none");
    }

    /// A3-core-11a: styling a DOM generated the hit-test tag ids at the end
    /// of the cascade, dropped them, and generated them again once the
    /// compact cache existed: two walks of every node. Once now, and the ids
    /// are the ones the second walk made.
    #[test]
    fn styling_a_dom_generates_its_tag_ids_once() {
        let page = || {
            Dom::create_body().with_child(
                Dom::create_div()
                    .with_css("overflow: scroll; width: 50px; height: 50px;")
                    .with_child(Dom::create_span_with_text("a label")),
            )
        };
        let mut styled = StyledDom::create_from_dom(page());
        assert_eq!(
            styled.get_css_property_cache().tag_id_passes.get(),
            1,
            "one walk for the tag ids of a new DOM"
        );
        let ids = styled.tag_ids_to_node_ids.clone();
        assert!(!ids.as_ref().is_empty(), "harness: the page has nodes with a tag");
        // The ids are what a pass over the finished DOM gives.
        let again = styled
            .get_css_property_cache()
            .generate_tag_ids(&styled.node_data.as_container(), &styled.node_hierarchy);
        assert_eq!(ids.as_ref(), &again[..]);

        styled.get_css_property_cache().tag_id_passes.reset();
        styled.restyle(Css::empty());
        assert_eq!(
            styled.get_css_property_cache().tag_id_passes.get(),
            1,
            "one walk for the tag ids of a restyle"
        );
    }

    /// A3-core-20 (A3-window-18): a styled DOM says whether the pass that
    /// makes a COMPOSED tree's inheritance and compact cache right is owed.
    /// One that came out of a cascade owes none; `append_child` owes one;
    /// the pass pays it. And the pass on a DOM that owes none builds the
    /// tables it had: skipping it there changes nothing a reader sees.
    #[test]
    fn a_styled_dom_owes_the_recompute_only_after_an_append() {
        let page = || {
            Dom::create_body()
                .with_css("color: red; font-size: 20px;")
                .with_child(Dom::create_div().with_css("width: 5px; padding: 3px;"))
        };
        let mut fresh = StyledDom::create_from_dom(page());
        assert!(!fresh.needs_recompute_after_append(), "one cascade: nothing owed");

        // The pass on such a DOM builds the compact cache it already has.
        let before = fresh
            .get_css_property_cache()
            .compact_cache
            .as_ref()
            .map(|c| (c.tier1_enums.clone(), c.tier2_dims.clone(), c.tier2_cold.clone(), c.tier2b_text.clone()))
            .expect("the cascade builds the compact cache");
        fresh.recompute_inheritance_and_compact_cache();
        let after = fresh
            .get_css_property_cache()
            .compact_cache
            .as_ref()
            .map(|c| (c.tier1_enums.clone(), c.tier2_dims.clone(), c.tier2_cold.clone(), c.tier2b_text.clone()))
            .expect("still there");
        assert!(before == after, "the pass on an uncomposed DOM changes no compact entry");
        assert!(!fresh.needs_recompute_after_append());

        // Composition owes the pass, and the pass pays it.
        let mut composed = StyledDom::create_from_dom(page());
        composed.append_child(StyledDom::create_from_dom(Dom::create_div().with_css("height: 9px;")));
        assert!(composed.needs_recompute_after_append(), "an appended subtree is not inherited into yet");
        composed.recompute_inheritance_and_compact_cache();
        assert!(!composed.needs_recompute_after_append());
    }
}

// -- diff first, step `style`: the cascade over the nodes a rebuild changed ------------------------
#[cfg(test)]
mod audit_diff_first_tests {
    use azul_css::props::property::CssPropertyType;

    use super::*;
    use crate::{
        diff::{adopt_changed_nodes, fingerprint_dom, same_tree_changes},
        dom::{Dom, NodeType},
        id::NodeId,
    };

    /// One section of [`page`]: which `k<kind>` class its heading and its box wear, which of
    /// [`PLACES`] styles the box (as a sheet hung on it in [`page`], as its own inline style
    /// in [`page_own`]), the text of its paragraph, and whether the paragraph holds a link.
    #[derive(Clone, PartialEq, Debug)]
    struct Section {
        kind: usize,
        place: usize,
        text: usize,
        link: bool,
    }

    const PLACES: [&str; 5] = [
        "",
        "position: relative; left: 4px;",
        "width: 50%; margin-left: 2em;",
        "color: #0000aa; font-size: 20px;",
        "line-height: 1.5em; cursor: pointer;",
    ];
    const WORDS: [&str; 4] = ["one", "two words", "", "Tuebingen"];

    /// `body > div.list[.dense] > section*`, a section being
    /// `div.box.k<n>[style] > (h1.k<n> > text), (p > text [, a > text])`: elements with UA
    /// defaults (the headings' `2em`, the link's colour, the paragraphs' margins) under a
    /// sheet with a `*` rule, class rules with inherited and font-relative values, a
    /// descendant and a child rule, a hover rule with an inherited value and - with `siblings`
    /// - two sibling rules.
    ///
    /// A section's place is a SHEET hung on its box here (`Dom::with_css`: scoped to the box,
    /// listed in the fingerprints' `sheets`): a rebuild that changes one is no change of
    /// nodes where they stand (`same_tree_changes` answers `None`), it is the reconciling
    /// rebuild's ([`tree_page`], `cascade_reusing`). [`page_own`] is the page for the exits
    /// that keep the DOM.
    fn page(sections: &[Section], dense: bool, siblings: bool) -> Dom {
        page_of(sections, dense, siblings, false, false)
    }

    /// [`page`] with each section's place as its box's OWN inline style (`NodeData::set_css`):
    /// what a rebuild changes "where the node stands", named by `same_tree_changes` and
    /// styled again by the cascade over the changed nodes. The walks of that cascade were
    /// written against [`page`] and never styled a place: a step that changed one found the
    /// two builds "not a change of nodes" and compared the untouched DOM with a new one.
    fn page_own(sections: &[Section], dense: bool, siblings: bool) -> Dom {
        page_of(sections, dense, siblings, true, false)
    }

    /// [`page`] whose links are LINKS: each `<a>` has an `href`, so the UA's `:link` colour is
    /// its own (`ua_css::is_link`: an `<a>` without an `href` is a placeholder, not `:link`,
    /// and inherits its colour). Added by the verifier for
    /// `a_state_that_hands_a_colour_down_costs_no_other_node_what_it_inherits`, the only test
    /// that builds it: every other walk keeps the page it was written against.
    fn page_with_hrefs(sections: &[Section]) -> Dom {
        page_of(sections, false, false, false, true)
    }

    /// [`page`] (`own` false: a place is a sheet hung on the box) and [`page_own`] (`own`
    /// true: it is the box's own style). `href`: the links have one ([`page_with_hrefs`]).
    fn page_of(sections: &[Section], dense: bool, siblings: bool, own: bool, href: bool) -> Dom {
        let text = |value: &str| {
            Dom::create_text_do_not_use_without_block_level_wrapper(value.to_string())
        };
        let mut list = Dom::create_div().with_class("list".into());
        if dense {
            list = list.with_class("dense".into());
        }
        for section in sections {
            let class = format!("k{}", section.kind);
            let mut block = Dom::create_div()
                .with_class("box".into())
                .with_class(class.clone().into());
            if !PLACES[section.place].is_empty() {
                if own {
                    block.root.set_css(PLACES[section.place]);
                } else {
                    block = block.with_css(PLACES[section.place]);
                }
            }
            let mut paragraph = Dom::create_node(NodeType::P).with_child(text(WORDS[section.text]));
            if section.link {
                let mut link = Dom::create_node(NodeType::A)
                    .with_class("link".into())
                    .with_child(text("link"));
                if href {
                    link = link
                        .with_attribute(crate::dom::AttributeType::Href("https://a.b/".into()));
                }
                paragraph = paragraph.with_child(link);
            }
            list = list.with_child(
                block
                    .with_child(
                        Dom::create_node(NodeType::H1)
                            .with_class(class.into())
                            .with_child(text("heading")),
                    )
                    .with_child(paragraph),
            );
        }
        let mut sheet = String::from(
            "* { box-sizing: border-box; } .list { padding: 2px; color: #222222; } \
             .box { padding: 4px; background: #eeeeee; } .dense .box { padding: 1px; } \
             .k1 { background: #ddddff; width: 200px; } \
             .k2 { color: #aa0000; font-size: 1.5em; line-height: 1.2em; } \
             .k3 { margin-top: 3px; text-align: center; } .box > .k3 { margin-left: 2em; } \
             .box:hover { background: #ffff00; color: #00aa00; } .link { cursor: help; }",
        );
        if siblings {
            sheet.push_str(" .k1 + .box { margin-left: 6px; } .k2 ~ .box { border-left-width: 2px; }");
        }
        Dom::create_body()
            .with_component_css(azul_css::css::Css::from_string(sheet.into()))
            .with_child(list)
    }

    fn sections(count: usize) -> Vec<Section> {
        (0..count)
            .map(|i| Section {
                kind: i % 4,
                place: 0,
                text: i % 4,
                link: i % 3 == 0,
            })
            .collect()
    }

    /// The step `kind` of the random walk applied to the page: a section's class (0, 1), its
    /// inline style (2, 3), its text (4), the list's class (5), three at once (6).
    fn mutate(sections: &mut [Section], dense: &mut bool, kind: usize, picks: [usize; 6]) {
        let n = sections.len();
        match kind {
            0 | 1 => sections[picks[0] % n].kind = picks[3] % 4,
            2 | 3 => sections[picks[0] % n].place = picks[3] % 5,
            4 => sections[picks[0] % n].text = picks[3] % 4,
            5 => *dense = !*dense,
            _ => {
                sections[picks[0] % n].kind = picks[3] % 4;
                sections[picks[1] % n].place = picks[4] % 5;
                sections[picks[2] % n].text = picks[5] % 4;
            }
        }
    }

    /// Every difference between what the cascades of `kept` and `full` answer, as lines: per
    /// node its matched and inherited rows, its computed inherited values, its compact
    /// entries and every property in the state it is in, at rest and hovered; then the
    /// tables beside the rows.
    fn differences(kept: &StyledDom, full: &StyledDom) -> Vec<String> {
        let (a, b) = (kept.get_css_property_cache(), full.get_css_property_cache());
        let mut out = Vec::new();
        if kept.node_data.as_ref() != full.node_data.as_ref() {
            out.push("the node data differs".to_string());
            return out;
        }
        let (ca, cb) = (
            a.compact_cache.as_ref().expect("a compact cache"),
            b.compact_cache.as_ref().expect("a compact cache"),
        );
        let hovered = StyledNodeState {
            hover: true,
            ..StyledNodeState::default()
        };
        for (index, node) in kept.node_data.as_ref().iter().enumerate() {
            let id = NodeId::new(index);
            if a.css_props.get_slice(index) != b.css_props.get_slice(index) {
                out.push(format!("node {index}: matched rules"));
            }
            if a.cascaded_props.get_slice(index) != b.cascaded_props.get_slice(index) {
                out.push(format!(
                    "node {index}: inherited row {:?} / {:?}",
                    a.cascaded_props.get_slice(index),
                    b.cascaded_props.get_slice(index)
                ));
            }
            if a.computed_values.values_for(index) != b.computed_values.values_for(index) {
                out.push(format!("node {index}: computed inherited values"));
            }
            if (
                ca.tier1_enums[index],
                ca.tier2_dims[index],
                ca.tier2_cold[index],
                ca.tier2b_text[index],
            ) != (
                cb.tier1_enums[index],
                cb.tier2_dims[index],
                cb.tier2_cold[index],
                cb.tier2b_text[index],
            ) {
                out.push(format!("node {index}: compact entries"));
            }
            let own = kept.styled_nodes.as_ref()[index].styled_node_state;
            for state in [own, StyledNodeState::default(), hovered] {
                for ty in CssPropertyType::ALL {
                    let (x, y) = (
                        a.get_property(node, &id, &state, ty),
                        b.get_property(node, &id, &state, ty),
                    );
                    if x != y {
                        out.push(format!("node {index}: {ty:?} is {x:?}, a new cascade says {y:?}"));
                    }
                }
            }
        }
        if ca.px_overflow != cb.px_overflow {
            out.push("the lengths beside the px slots".to_string());
        }
        if a.css_wide_values != b.css_wide_values {
            out.push("the css-wide values".to_string());
        }
        if a.inline_overruled != b.inline_overruled {
            out.push("the overruled inline declarations".to_string());
        }
        if a.global_css_props != b.global_css_props {
            out.push("the `*` bucket".to_string());
        }
        if kept.tag_ids_to_node_ids != full.tag_ids_to_node_ids {
            out.push("the hit-test tags".to_string());
        }
        if kept.styled_nodes != full.styled_nodes {
            out.push("the node states".to_string());
        }
        out
    }

    /// Writes the nodes `fresh` changed onto `kept` and styles them again as the window does:
    /// the cascade over the nodes the changes reach, and the whole cascade (with the prune a
    /// created DOM has) where that says no. Answers how many nodes the changes reach and
    /// whether the cascade over them ran; `None` when the two builds are alike - and when the
    /// rebuild is no change of nodes where they stand (another tree, a sheet hung on a node
    /// that changed): `kept` is then untouched, and the caller must not take it for rebuilt.
    fn rebuild_onto(kept: &mut StyledDom, old: &Dom, fresh: &Dom) -> Option<(usize, bool)> {
        let changed = same_tree_changes(&fingerprint_dom(old).0, &fingerprint_dom(fresh).0)?;
        let _ = adopt_changed_nodes(kept.node_data.as_mut(), fresh, &changed);
        let reached = kept.nodes_reached_by_changes(&changed);
        let over_the_nodes = kept.restyle_changed_nodes(&reached);
        if !over_the_nodes {
            let css = kept.get_css_property_cache().retained_author_css.clone();
            kept.restyle(css);
            kept.get_css_property_cache_mut().prune_compact_normal_props();
        }
        Some((reached.iter().filter(|r| **r).count(), over_the_nodes))
    }

    /// A random walk of changes to one page: after each, the kept DOM with the changed nodes
    /// styled again answers what a DOM styled from scratch answers, row by row and property
    /// by property. With `hover`, section 1's box is hovered in both (its hover rule hands
    /// its children a colour); with `siblings` the sheet has sibling rules, so a changed box
    /// reaches every section and most steps take the whole cascade. Answers how many steps
    /// the cascade over the changed nodes ran for.
    fn walk(hover: bool, siblings: bool) -> usize {
        let mut over_the_nodes = 0;
        for seed in [1u64, 7, 20261121] {
            let mut spec = sections(9);
            let mut dense = false;
            let mut old = page_own(&spec, dense, siblings);
            let mut kept = StyledDom::create_from_dom(page_own(&spec, dense, siblings));
            // Pre-order: body 0, the list 1, section 0 (a box, its heading and text, its
            // paragraph and text, its link and text) 2..=8, section 1's box 9.
            let hover_node = NodeId::new(9);
            let raise = |dom: &mut StyledDom| {
                if hover {
                    dom.styled_nodes.as_container_mut()[hover_node]
                        .styled_node_state
                        .hover = true;
                    let css = dom.get_css_property_cache().retained_author_css.clone();
                    dom.restyle(css);
                    dom.get_css_property_cache_mut().prune_compact_normal_props();
                }
            };
            raise(&mut kept);
            let mut random = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
            let mut next = |below: u64| {
                random = random
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                ((random >> 33) % below) as usize
            };
            for step in 0..40 {
                let kind = next(7);
                let picks = [next(64), next(64), next(64), next(64), next(64), next(64)];
                mutate(&mut spec, &mut dense, kind, picks);
                let fresh = page_own(&spec, dense, siblings);
                let rebuilt = rebuild_onto(&mut kept, &old, &fresh);
                // A step the kept DOM was not rebuilt for must be a step that changed
                // nothing (a pick that names the value a section has): anything else left
                // `kept` as it was and the comparison below would blame the cascade.
                assert!(
                    rebuilt.is_some() || fingerprint_dom(&old).0 == fingerprint_dom(&fresh).0,
                    "harness: seed {seed}, step {step} (kind {kind}, picks {picks:?}) changed \
                     the page and is no change of nodes where they stand"
                );
                if rebuilt.is_some_and(|(_, ran)| ran) {
                    over_the_nodes += 1;
                }
                let mut full = StyledDom::create_from_dom(page_own(&spec, dense, siblings));
                raise(&mut full);
                let found = differences(&kept, &full);
                assert!(
                    found.is_empty(),
                    "seed {seed}, step {step} (kind {kind}, picks {picks:?}, hover {hover}, \
                     siblings {siblings}): {} differences, the first: {:?}",
                    found.len(),
                    found.first()
                );
                old = fresh;
            }
        }
        over_the_nodes
    }

    /// How to read a failure: the message names the seed, the step and its kind ([`mutate`])
    /// and the first difference - which row or which property of which node.
    #[test]
    fn the_cascade_over_changed_nodes_answers_what_a_new_cascade_answers() {
        let ran = walk(false, false);
        assert!(ran >= 40, "premise: many of the 120 steps style their own nodes ({ran} did)");
    }

    #[test]
    fn the_cascade_over_changed_nodes_answers_what_a_new_cascade_answers_under_a_hover() {
        let ran = walk(true, false);
        assert!(ran >= 40, "premise: many of the 120 steps style their own nodes ({ran} did)");
    }

    /// Under a sheet with sibling rules: a changed box reaches the sections beside it, a
    /// changed text only its paragraph.
    #[test]
    fn the_cascade_over_changed_nodes_answers_what_a_new_cascade_answers_beside_sibling_rules() {
        let ran = walk(false, true);
        assert!(ran >= 1, "premise: a changed text styles its own nodes");
    }

    /// The cascade runs for the nodes a change reaches and for no other: one section's class
    /// is its box and what is below it - a heading and a paragraph with their texts, 5 nodes
    /// - in a page of 8 sections and in one of 32.
    #[test]
    fn one_changed_class_styles_the_subtree_of_its_node() {
        let cost = |count: usize, siblings: bool| {
            let mut spec = sections(count);
            for section in &mut spec {
                section.kind = 0;
                section.link = false;
            }
            let old = page(&spec, false, siblings);
            let mut kept = StyledDom::create_from_dom(page(&spec, false, siblings));
            spec[count / 2].kind = 3;
            let fresh = page(&spec, false, siblings);
            let before = nodes_cascaded_on_this_thread();
            let (reached, ran) = rebuild_onto(&mut kept, &old, &fresh).expect("a class changed");
            (reached, ran, nodes_cascaded_on_this_thread() - before)
        };
        assert_eq!(cost(8, false), (5, true, 5), "8 sections: the section's five nodes");
        assert_eq!(cost(32, false), (5, true, 5), "32 sections: the same five nodes");
        // With a sibling rule in the sheet the sections after the changed one can match
        // another rule: the change names the subtree of the list, which is more than half of
        // the page, and the whole cascade runs (every node of the page: 2 + 5 per section).
        assert_eq!(cost(8, true), (1 + 8 * 5, false, 2 + 8 * 5));
    }

    /// diff-first-19: a state that hands its children a value reaches every node below it
    /// that declares none of its own, in BOTH stores (what `get_property` answers and the
    /// compact entry the text is painted from), and costs no OTHER node an inherited value.
    /// Section 1's box comes under the pointer (`.box:hover { color }`): its heading, its
    /// paragraph and their texts take the colour; the list keeps its colour, and the link of
    /// section 0 keeps the UA's link colour, which its text inherits - a colour that is not
    /// black and that no rule of the page declares. First on a DOM cascaded at rest that is
    /// given the state flag by flag (what a full rebuild under the pointer does:
    /// `let_raised_states_in`), then for nodes below the hovered box styled again on their
    /// own (a rebuild that changed them), then as the pointer moves on
    /// (`restyle_on_state_change`): to a section whose heading has a colour of its own, to a
    /// section with a link, and away.
    ///
    /// Two defects, one after the other:
    /// 1. before `8e9ead705` both paths ran the inheritance pass over the rows as they lay,
    ///    after the cascade had pruned the resting rows the compact cache holds - the UA's
    ///    defaults among them - so every node whose colour came from such a row lost it: the
    ///    link read the list's colour. Mutation: `recascade_subtrees` replaced by
    ///    `compute_inherited_values` over the whole DOM: "the list and the link keep their
    ///    colours" fails for the link and its text (#222222 for #0000ee).
    /// 2. `8e9ead705` styled the subtree by the cascade over named nodes, whose inheritance
    ///    pass reads the rows BEFORE their prune: a child's row holds a copy of what its
    ///    parent holds at rest, and the pass applied the copy over the hover colour the
    ///    child had just inherited. The compact entries (copied slot by slot from the
    ///    parent) had the hover colour, `get_property` the resting one. Mutations, each
    ///    named at the assertion that fails for it:
    ///    - the copy is applied again (`apply_cascade_properties` without `from_a_state`):
    ///      "the hovered box hands its text its hover colour", and every node below the box;
    ///    - a node does not hand on what it took from above (`hands_on` without the types
    ///      of `held_above`): the TEXTS below the box (grandchildren) keep the list's;
    ///    - the kept parent of a named node is asked nothing (`types_shown_from_a_state`
    ///      answers none): "a paragraph styled again below the hovered box";
    ///    - it is asked for its own state only (no walk up): "its text styled again alone";
    ///    - a UA default of the type is passed over like a copy (`p.ua_origin ||` dropped):
    ///      "the link below a hovered box keeps the UA's colour";
    ///    - the node that LEFT the state is not styled again: "the pointer left".
    #[test]
    // the index addresses more than one list and is the number the failure message names
    #[allow(clippy::needless_range_loop)]
    fn a_state_that_hands_a_colour_down_costs_no_other_node_what_it_inherits() {
        use crate::styled_dom::HoverChange;

        // Pre-order: body 0, the list 1; section 0 (with a link): box 2, heading 3 and its
        // text 4, paragraph 5 and its text 6, the link 7 and its text 8; section 1: box 9,
        // heading 10 and its text 11, paragraph 12 and its text 13; section 2 (class `k2`,
        // which gives the box and the heading a colour): box 14, heading 15 and its text
        // 16, paragraph 17 and its text 18; section 3 (with a link): box 19, heading 20 and
        // its text 21, paragraph 22 and its text 23, the link 24 and its text 25. The texts
        // watched first are the paragraphs': a paragraph has no colour of its own, so its
        // text shows what the box hands down.
        const HOVER: (u8, u8, u8) = (0x00, 0xaa, 0x00);
        const LIST: (u8, u8, u8) = (0x22, 0x22, 0x22);
        const K2: (u8, u8, u8) = (0xaa, 0x00, 0x00);
        const LINK: (u8, u8, u8) = (0x00, 0x00, 0xee);
        let (list, link, link_text) = (NodeId::new(1), NodeId::new(7), NodeId::new(8));
        let (first, first_text) = (NodeId::new(9), NodeId::new(13));
        let (second, second_text) = (NodeId::new(14), NodeId::new(18));
        let third = NodeId::new(19);
        let colour = |dom: &StyledDom, node: NodeId| {
            let state = dom.styled_nodes.as_ref()[node.index()].styled_node_state;
            dom.get_css_property_cache()
                .get_property(
                    &dom.node_data.as_ref()[node.index()],
                    &node,
                    &state,
                    &CssPropertyType::TextColor,
                )
                .cloned()
        };
        // The two stores: what `get_property` answers for the state the node is in, and
        // the compact entry (what the text is painted from).
        let both = |dom: &StyledDom, node: usize| {
            let slow = match colour(dom, NodeId::new(node)) {
                Some(CssProperty::TextColor(v)) => {
                    v.get_property().map(|c| (c.inner.r, c.inner.g, c.inner.b))
                }
                _ => None,
            };
            let raw = dom
                .get_css_property_cache()
                .compact_cache
                .as_ref()
                .map_or(0, |compact| compact.get_text_color_raw(node));
            let fast = (raw != 0).then_some(((raw >> 24) as u8, (raw >> 16) as u8, (raw >> 8) as u8));
            (slow, fast)
        };
        let in_both = |c: (u8, u8, u8)| (Some(c), Some(c));
        // Premise corrected by the verifier: an `<a>` without `href` is not `:link` and gets no
        // UA colour (`ua_css::is_link`): on `page` the link answered the list's #222222 and
        // the third premise below could not hold. The page is `page` with an `href` on each
        // link.
        let mut dom = StyledDom::create_from_dom(page_with_hrefs(&sections(4)));
        let count = dom.node_data.as_ref().len();
        assert_eq!(count, 26, "premise: the page is the one numbered above");
        let at_rest = [colour(&dom, list), colour(&dom, link), colour(&dom, link_text)];
        assert!(at_rest[0].is_some(), "premise: the list has a colour of its own");
        assert!(
            at_rest[1].is_some() && at_rest[1] == at_rest[2],
            "premise: the link's text inherits the link's colour ({at_rest:?})"
        );
        assert_ne!(at_rest[1], at_rest[0], "premise: the link's colour is the UA's, not the list's");
        let (first_rest, second_rest) = (colour(&dom, first_text), colour(&dom, second_text));
        let rest: Vec<_> = (0..count).map(|node| both(&dom, node)).collect();
        for node in 10..=13 {
            assert_eq!(rest[node], in_both(LIST), "premise: node {node} rests at the list's colour");
        }
        assert_eq!(rest[18], in_both(K2), "premise: section 2's text rests at its box's colour");
        assert_eq!(rest[7], in_both(LINK), "premise: the link rests at the UA's colour");

        // The state comes back after a cascade at rest, as in a full rebuild.
        dom.styled_nodes.as_container_mut()[first].styled_node_state.hover = true;
        dom.let_raised_states_in();
        let hovered_text = colour(&dom, first_text);
        assert_ne!(hovered_text, first_rest, "the hovered box hands its text its hover colour");
        for node in 10..=13 {
            assert_eq!(
                both(&dom, node),
                in_both(HOVER),
                "node {node} below the hovered box: the hover colour, in both stores"
            );
        }
        assert_eq!(
            [colour(&dom, list), colour(&dom, link), colour(&dom, link_text)],
            at_rest,
            "after the state came: the list and the link (with its text) keep their colours"
        );
        assert_eq!(colour(&dom, second_text), second_rest, "a text beside it stays at rest");

        // Nodes below the hovered box styled again on their own (a rebuild changed them):
        // their parent is KEPT, and what it shows from the state is read from the cache.
        let mut named = vec![false; count];
        named[12] = true;
        named[13] = true;
        assert!(dom.restyle_changed_nodes(&named), "harness: the cascade over two nodes runs");
        for node in [12, 13] {
            assert_eq!(
                both(&dom, node),
                in_both(HOVER),
                "a paragraph styled again below the hovered box: node {node}"
            );
        }
        // The text alone: its parent, the paragraph, is in no state and hands on what the
        // box hands it.
        let mut named = vec![false; count];
        named[13] = true;
        assert!(dom.restyle_changed_nodes(&named), "harness: the cascade over one node runs");
        assert_eq!(both(&dom, 13), in_both(HOVER), "its text styled again alone");

        // The pointer moves on to the next section, whose heading has a colour of its own.
        let _ = dom.restyle_on_state_change(
            None,
            Some(HoverChange {
                left_nodes: vec![first],
                entered_nodes: vec![second],
            }),
            None,
        );
        assert_eq!(colour(&dom, first_text), first_rest, "the text the pointer left is at rest");
        assert_eq!(colour(&dom, second_text), hovered_text, "the text it entered has the colour");
        for node in 10..=13 {
            assert_eq!(both(&dom, node), rest[node], "the pointer left: node {node} is at rest again");
        }
        for node in [17, 18] {
            assert_eq!(both(&dom, node), in_both(HOVER), "the paragraph it entered: node {node}");
        }
        for node in [15, 16] {
            assert_eq!(
                both(&dom, node),
                in_both(K2),
                "a heading with a colour of its own keeps it below a hovered box, and its \
                 text with it: node {node}"
            );
        }
        assert_eq!(
            [colour(&dom, list), colour(&dom, link), colour(&dom, link_text)],
            at_rest,
            "after the pointer moved: the list and the link (with its text) keep their colours"
        );

        // On to the section with a link: the paragraph and its text take the colour, the
        // link (a grandchild of the box) keeps the UA's, which is its own, and its text
        // inherits the link's.
        let _ = dom.restyle_on_state_change(
            None,
            Some(HoverChange {
                left_nodes: vec![second],
                entered_nodes: vec![third],
            }),
            None,
        );
        for node in 15..=18 {
            assert_eq!(both(&dom, node), rest[node], "the pointer left: node {node} is at rest again");
        }
        for node in [20, 21, 22, 23] {
            assert_eq!(both(&dom, node), in_both(HOVER), "below the third box: node {node}");
        }
        for node in [24, 25] {
            assert_eq!(
                both(&dom, node),
                in_both(LINK),
                "the link below a hovered box keeps the UA's colour, and its text with it: \
                 node {node}"
            );
        }

        // And away: every node answers what it answered before the pointer came.
        let _ = dom.restyle_on_state_change(
            None,
            Some(HoverChange {
                left_nodes: vec![third],
                entered_nodes: Vec::new(),
            }),
            None,
        );
        for node in 0..count {
            assert_eq!(both(&dom, node), rest[node], "the pointer is gone: node {node}");
        }
    }

    /// The same for a property the compact cache has no slot the prune trusts for
    /// (`cursor`): the child's copy of the resting value SURVIVES the prune, and
    /// `get_property` finds the row before it asks the inherited values. A bar under the
    /// pointer hands its paragraph and the text in it `cursor: pointer`; they answer what
    /// the bar answers, and the resting cursor again once the pointer is gone. With four
    /// more nodes beside the bar the cascade over the bar's three nodes runs; without them
    /// they are more than half of the DOM and the whole cascade runs in place: both ways.
    ///
    /// Mutation: the lookup's third tier answers the row's resting copy where the node
    /// holds the type as an inherited value (the branch on `CssPropertyOrigin::Inherited`
    /// in `get_declared_property_ranked` removed): the paragraph and its text answer
    /// `default` under the hovered bar.
    #[test]
    fn a_state_hands_down_a_property_whose_copy_survives_the_prune() {
        use crate::styled_dom::HoverChange;

        for beside in [4_usize, 0] {
            let text = Dom::create_text_do_not_use_without_block_level_wrapper("x".to_string());
            let mut body = Dom::create_body()
                .with_component_css(azul_css::css::Css::from_string(
                    ".bar { cursor: default; } .bar:hover { cursor: pointer; }".into(),
                ))
                .with_child(
                    Dom::create_div()
                        .with_class("bar".into())
                        .with_child(Dom::create_node(NodeType::P).with_child(text)),
                );
            for _ in 0..beside {
                body = body.with_child(Dom::create_div());
            }
            // Pre-order: body 0, the bar 1, its paragraph 2, the text 3, then the others.
            let mut dom = StyledDom::create_from_dom(body);
            assert_eq!(dom.node_data.as_ref().len(), 4 + beside, "premise: the nodes as numbered");
            let bar = NodeId::new(1);
            let cursor = |dom: &StyledDom, node: usize| {
                let id = NodeId::new(node);
                let state = dom.styled_nodes.as_ref()[node].styled_node_state;
                dom.get_css_property_cache()
                    .get_property(&dom.node_data.as_ref()[node], &id, &state, &CssPropertyType::Cursor)
                    .cloned()
            };
            let resting = cursor(&dom, 1);
            assert!(resting.is_some(), "premise: the bar declares a cursor");
            for node in [2, 3] {
                assert_eq!(cursor(&dom, node), resting, "premise: node {node} inherits the bar's");
            }

            dom.styled_nodes.as_container_mut()[bar].styled_node_state.hover = true;
            dom.let_raised_states_in();
            let hovered = cursor(&dom, 1);
            assert_ne!(hovered, resting, "premise: the hovered bar has another cursor");
            for node in [2, 3] {
                assert_eq!(
                    cursor(&dom, node),
                    hovered,
                    "{beside} nodes beside the bar: node {node} takes the hovered bar's cursor"
                );
            }

            let _ = dom.restyle_on_state_change(
                None,
                Some(HoverChange {
                    left_nodes: vec![bar],
                    entered_nodes: Vec::new(),
                }),
                None,
            );
            for node in [1, 2, 3] {
                assert_eq!(
                    cursor(&dom, node),
                    resting,
                    "{beside} nodes beside the bar: node {node} is at rest again"
                );
            }
        }
    }

    /// Without a sibling rule in the sheet a changed node reaches itself and its descendants.
    #[test]
    fn a_change_reaches_the_node_and_what_is_below_it() {
        let dom = |class: &'static str| {
            Dom::create_body()
                .with_component_css(azul_css::css::Css::from_string(
                    ".a { color: red; } .b { color: blue; }".into(),
                ))
                .with_child(Dom::create_div().with_child(Dom::create_div()))
                .with_child(
                    Dom::create_div()
                        .with_class(class.into())
                        .with_child(Dom::create_div().with_child(Dom::create_div())),
                )
                .with_child(Dom::create_div())
        };
        // Pre-order: body 0, div 1, div 2, div.{class} 3, div 4, div 5, div 6.
        let mut kept = StyledDom::create_from_dom(dom("a"));
        let before = nodes_cascaded_on_this_thread();
        let reached = rebuild_onto(&mut kept, &dom("a"), &dom("b"));
        assert_eq!(reached, Some((3, true)), "the node and the two below it");
        assert_eq!(nodes_cascaded_on_this_thread() - before, 3);
        assert_eq!(
            kept.nodes_reached_by_changes(&[3]),
            vec![false, false, false, true, true, true, false]
        );
        let found = differences(&kept, &StyledDom::create_from_dom(dom("b")));
        assert!(found.is_empty(), "{found:?}");
    }

    /// A custom property anywhere is the whole cascade's: the cascade over changed nodes
    /// says no and changes nothing.
    #[test]
    fn a_dom_with_a_custom_property_is_left_to_the_whole_cascade() {
        let dom = |class: &'static str| {
            Dom::create_body()
                .with_component_css(azul_css::css::Css::from_string(
                    "body { --ink: red; } .a { color: var(--ink); } .b { color: blue; }".into(),
                ))
                .with_child(Dom::create_div().with_class(class.into()))
        };
        let mut kept = StyledDom::create_from_dom(dom("a"));
        let changed =
            same_tree_changes(&fingerprint_dom(&dom("a")).0, &fingerprint_dom(&dom("b")).0)
                .expect("the class changed");
        let _ = adopt_changed_nodes(kept.node_data.as_mut(), &dom("b"), &changed);
        let reached = kept.nodes_reached_by_changes(&changed);
        let epoch = kept.get_css_property_cache().cascade_epoch;
        assert!(!kept.restyle_changed_nodes(&reached));
        assert_eq!(kept.get_css_property_cache().cascade_epoch, epoch, "nothing was cascaded");
    }

    // -- diff first, step `tree`: the first cascade of a DOM whose tree changed ---------------

    /// [`page`], with a second sheet of structural rules (`:first-child`, `:nth-child`,
    /// `:last-child`) when `structural`: a section that moves among its siblings can then
    /// match another set of rules.
    fn tree_page(spec: &[Section], dense: bool, siblings: bool, structural: bool) -> Dom {
        let dom = page(spec, dense, siblings);
        if structural {
            dom.with_component_css(azul_css::css::Css::from_string(
                ".box:first-child { border-top-width: 3px; } .box:nth-child(even) > p { color: \
                 #005500; } .box:last-child { margin-bottom: 9px; }"
                    .into(),
            ))
        } else {
            dom
        }
    }

    /// The step `kind` of the random walk of a page whose TREE changes: a section comes (7),
    /// goes (8), moves (9), gains or loses its link (10: a subtree inside a paragraph), a
    /// section comes while another one changes its class (11); the kinds below 7 are
    /// [`mutate`]'s (nodes change where they stand).
    fn mutate_tree(spec: &mut Vec<Section>, dense: &mut bool, kind: usize, picks: [usize; 6]) {
        let n = spec.len();
        let new_section = Section {
            kind: picks[3] % 4,
            place: picks[4] % 5,
            text: picks[5] % 4,
            link: picks[2].is_multiple_of(2),
        };
        match kind {
            7 => spec.insert(picks[0] % (n + 1), new_section),
            8 => {
                if n > 3 {
                    let _ = spec.remove(picks[0] % n);
                }
            }
            9 => {
                let section = spec.remove(picks[0] % n);
                spec.insert(picks[1] % n, section);
            }
            10 => {
                let at = picks[0] % n;
                spec[at].link = !spec[at].link;
            }
            11 => {
                spec[picks[1] % n].kind = picks[4] % 4;
                spec.insert(picks[0] % (n + 1), new_section);
            }
            _ => mutate(spec, dense, kind, picks),
        }
    }

    /// The first cascade of `fresh` as a rebuild that changed the tree runs it: the DOM is
    /// flattened unstyled, the reconciliation pairs its nodes with those of `old`, the hover
    /// follows the node it is on (`hovered`, a node of `old`), and the cascade keeps the rows
    /// of the nodes that are what they were. Where it keeps none, the whole cascade at rest
    /// and then the states, as a rebuild always ran. Answers the DOM, whether rows were kept,
    /// and the hovered node in it.
    fn rebuild_reusing(
        old: &StyledDom,
        old_dom: &Dom,
        fresh: Dom,
        hovered: Option<NodeId>,
    ) -> (StyledDom, bool, Option<NodeId>) {
        // The sheets that hang on the nodes, read off the two `Dom`s before they are flat.
        let old_sheets = fingerprint_dom(old_dom).0.sheets;
        let new_sheets = fingerprint_dom(&fresh).0.sheets;
        let mut new = StyledDom::unstyled_from_dom_inheriting(fresh, None, &[], Vec::new());
        let no_layout = crate::OrderedMap::default();
        let moves = crate::diff::reconcile_dom(
            old.node_data.as_ref(),
            new.node_data.as_ref(),
            old.node_hierarchy.as_ref(),
            new.node_hierarchy.as_ref(),
            &no_layout,
            &no_layout,
            crate::dom::DomId::ROOT_ID,
            crate::task::Instant::now(),
            crate::diff::NodeIdentityConfig::default(),
        )
        .node_moves;
        let mut old_of_new: Vec<Option<NodeId>> = vec![None; new.node_data.as_ref().len()];
        for m in &moves {
            old_of_new[m.new_node_id.index()] = Some(m.old_node_id);
        }
        let hovered = hovered.and_then(|node| {
            moves
                .iter()
                .find(|m| m.old_node_id == node)
                .map(|m| m.new_node_id)
        });
        if let Some(node) = hovered {
            new.styled_nodes.as_container_mut()[node]
                .styled_node_state
                .hover = true;
        }
        let kept = new
            .cascade_reusing(old, &old_of_new, &old_sheets, &new_sheets)
            .is_some();
        if !kept {
            new.cascade_at_rest();
            if hovered.is_some() {
                let css = new.get_css_property_cache().retained_author_css.clone();
                new.restyle(css);
                new.get_css_property_cache_mut().prune_compact_normal_props();
            }
        }
        (new, kept, hovered)
    }

    /// A random walk of rebuilds that change the tree of one page: after each, the DOM whose
    /// cascade kept the rows of the DOM before answers what a DOM styled from scratch
    /// answers, row by row and property by property ([`differences`]). With `hover`, a box
    /// is hovered and stays hovered while the reconciliation finds it again. Answers how
    /// many of the rebuilds kept rows.
    fn tree_walk(hover: bool, siblings: bool, structural: bool) -> usize {
        let mut kept_rows = 0;
        for seed in [1u64, 7, 20261121] {
            let mut spec = sections(9);
            let mut dense = false;
            let mut old = StyledDom::create_from_dom(tree_page(&spec, dense, siblings, structural));
            let mut old_dom = tree_page(&spec, dense, siblings, structural);
            // Pre-order: body 0, the list 1, section 0 (7 nodes) 2..=8, section 1's box 9.
            let mut hovered = hover.then(|| NodeId::new(9));
            let raise = |dom: &mut StyledDom, node: Option<NodeId>| {
                if let Some(node) = node {
                    dom.styled_nodes.as_container_mut()[node]
                        .styled_node_state
                        .hover = true;
                    let css = dom.get_css_property_cache().retained_author_css.clone();
                    dom.restyle(css);
                    dom.get_css_property_cache_mut().prune_compact_normal_props();
                }
            };
            raise(&mut old, hovered);
            let mut random = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
            let mut next = |below: u64| {
                random = random
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                ((random >> 33) % below) as usize
            };
            for step in 0..40 {
                let kind = next(12);
                let picks = [next(64), next(64), next(64), next(64), next(64), next(64)];
                mutate_tree(&mut spec, &mut dense, kind, picks);
                let (new, kept, now_hovered) = rebuild_reusing(
                    &old,
                    &old_dom,
                    tree_page(&spec, dense, siblings, structural),
                    hovered,
                );
                if kept {
                    kept_rows += 1;
                }
                let mut full =
                    StyledDom::create_from_dom(tree_page(&spec, dense, siblings, structural));
                raise(&mut full, now_hovered);
                let found = differences(&new, &full);
                assert!(
                    found.is_empty(),
                    "seed {seed}, step {step} (kind {kind}, picks {picks:?}, hover {hover}, \
                     siblings {siblings}, structural {structural}, rows kept {kept}): {} \
                     differences, the first: {:?}",
                    found.len(),
                    found.first()
                );
                old = new;
                old_dom = tree_page(&spec, dense, siblings, structural);
                hovered = now_hovered;
            }
        }
        kept_rows
    }

    /// How to read a failure: the message names the seed, the step and its kind
    /// ([`mutate_tree`]), whether the rebuild kept rows, and the first difference - which row
    /// or which property of which node of the new DOM.
    #[test]
    fn a_cascade_that_keeps_rows_across_a_changed_tree_answers_what_a_new_cascade_answers() {
        let kept = tree_walk(false, false, false);
        assert!(kept >= 20, "premise: many of the 120 rebuilds keep rows ({kept} did)");
    }

    #[test]
    fn a_cascade_that_keeps_rows_across_a_changed_tree_answers_the_same_under_a_hover() {
        let kept = tree_walk(true, false, false);
        assert!(kept >= 20, "premise: many of the 120 rebuilds keep rows ({kept} did)");
    }

    /// Under sibling rules a section that comes or goes changes what the sections after it
    /// match: their rows are not kept, the ones before keep theirs.
    #[test]
    fn a_cascade_that_keeps_rows_across_a_changed_tree_answers_the_same_beside_sibling_rules() {
        let _ = tree_walk(false, true, false);
        let _ = tree_walk(true, true, false);
    }

    /// Under `:first-child` / `:nth-child` / `:last-child` rules a section that moves among
    /// its siblings is cascaded again.
    #[test]
    fn a_cascade_that_keeps_rows_across_a_changed_tree_answers_the_same_under_structural_rules() {
        let _ = tree_walk(false, false, true);
        let _ = tree_walk(false, true, true);
    }

    /// `(nodes of the new DOM, nodes the cascade ran for)` when `added` sections come at the
    /// end of a page of `count` sections without links (5 nodes a section, 2 above them).
    fn cost_of_sections_that_come(count: usize, added: usize, structural: bool) -> (usize, usize) {
        let mut spec = sections(count);
        for section in &mut spec {
            section.link = false;
        }
        let old = StyledDom::create_from_dom(tree_page(&spec, false, false, structural));
        let old_dom = tree_page(&spec, false, false, structural);
        for i in 0..added {
            spec.push(Section {
                kind: i % 4,
                place: 0,
                text: i % 4,
                link: false,
            });
        }
        let before = nodes_cascaded_on_this_thread();
        let (new, kept, _) =
            rebuild_reusing(&old, &old_dom, tree_page(&spec, false, false, structural), None);
        assert!(kept, "premise: the rebuild keeps rows");
        (
            new.node_data.as_ref().len(),
            nodes_cascaded_on_this_thread() - before,
        )
    }

    /// Sections that come style their own nodes: 10 new sections are 50 nodes, in a page of
    /// 30 sections and in one of 120. (The bound is exact: a section that comes at the end
    /// leaves every other node its data, its ancestors and - without a structural rule - all
    /// a rule can read of its place.)
    #[test]
    fn sections_that_come_are_cascaded_and_no_other_node() {
        assert_eq!(cost_of_sections_that_come(30, 10, false), (2 + 40 * 5, 50));
        assert_eq!(cost_of_sections_that_come(120, 10, false), (2 + 130 * 5, 50));
        // With `:last-child` in the sheet the section that was last is no longer: it is
        // cascaded with the ones that came (5 nodes more), whatever the size of the page.
        assert_eq!(cost_of_sections_that_come(30, 10, true), (2 + 40 * 5, 55));
        assert_eq!(cost_of_sections_that_come(120, 10, true), (2 + 130 * 5, 55));
    }

    /// The rows are compared on the arenas, not on what an earlier build said: a node of the
    /// old DOM whose style a callback wrote since (here: by hand) does not hand its rows to
    /// the node the next build makes at its place.
    #[test]
    fn a_node_written_since_its_build_does_not_hand_on_its_rows() {
        let spec = sections(6);
        let mut old = StyledDom::create_from_dom(tree_page(&spec, false, false, false));
        // Section 0's box (node 2) gets a style of its own, as `set_node_style` writes it
        // (the node's own style, not a sheet hung on it: no fingerprint of the build shows it).
        let written = crate::dom::NodeData::create_div().with_css("width: 77px;").style.clone();
        old.node_data.as_container_mut()[NodeId::new(2)].style = written;
        let mut grown = spec.clone();
        grown.push(spec[0].clone());
        let before = nodes_cascaded_on_this_thread();
        let (new, kept, _) = rebuild_reusing(
            &old,
            &tree_page(&spec, false, false, false),
            tree_page(&grown, false, false, false),
            None,
        );
        assert!(kept, "premise: the rebuild keeps rows");
        // The written box and its subtree (7 nodes: section 0 has a link) and the section
        // that came (7 nodes). Had the box handed on its rows, only the 7 that came.
        assert_eq!(nodes_cascaded_on_this_thread() - before, 14);
        let found = differences(
            &new,
            &StyledDom::create_from_dom(tree_page(&grown, false, false, false)),
        );
        assert!(found.is_empty(), "{found:?}");
    }

    /// A sheet hung on a node (`Dom::with_css`, the usual inline style) is no field of the
    /// node in the arena: the node whose sheet changed is cascaded again, with what is below
    /// it, because the sheets are compared by what the two `Dom`s said hangs where.
    #[test]
    fn a_node_whose_own_sheet_changed_is_cascaded_again() {
        let mut spec = sections(6);
        for section in &mut spec {
            section.link = false;
        }
        let old_dom = tree_page(&spec, false, false, false);
        let old = StyledDom::create_from_dom(tree_page(&spec, false, false, false));
        // Section 2's box gets `PLACES[1]` through `Dom::with_css`, and a section comes.
        spec[2].place = 1;
        spec.push(spec[0].clone());
        let before = nodes_cascaded_on_this_thread();
        let (new, kept, _) =
            rebuild_reusing(&old, &old_dom, tree_page(&spec, false, false, false), None);
        assert!(kept, "premise: the rebuild keeps rows");
        assert_eq!(
            nodes_cascaded_on_this_thread() - before,
            10,
            "the section whose sheet changed (5 nodes) and the one that came (5 nodes)"
        );
        let found = differences(
            &new,
            &StyledDom::create_from_dom(tree_page(&spec, false, false, false)),
        );
        assert!(found.is_empty(), "{found:?}");
    }

    /// The two halves of the creation are the creation: a DOM flattened unstyled and then
    /// cascaded at rest is the DOM `create_from_dom` makes.
    #[test]
    fn an_unstyled_dom_cascaded_at_rest_is_the_dom_the_creation_makes() {
        let spec = sections(7);
        let mut halves =
            StyledDom::unstyled_from_dom_inheriting(tree_page(&spec, true, true, true), None, &[], Vec::new());
        assert!(halves.tag_ids_to_node_ids.as_ref().is_empty(), "premise: no cascade ran yet");
        halves.cascade_at_rest();
        let whole = StyledDom::create_from_dom(tree_page(&spec, true, true, true));
        let found = differences(&halves, &whole);
        assert!(found.is_empty(), "{found:?}");
        assert!(halves == whole, "every field, the tables as they lie");
    }
}
