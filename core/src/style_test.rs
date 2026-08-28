#[cfg(test)]
#[allow(clippy::pedantic, clippy::nursery, clippy::too_many_lines)]
mod autotest_generated {
    use azul_css::{
        css::{CssNthChildPattern, CssScopeRange, NodeTypeTag},
        OptionString,
    };

    use super::*;
    use crate::{
        dom::{AttributeNameValue, AttributeType, NodeType},
        id::Node,
    };

    // ---------------------------------------------------------------------
    // helpers
    // ---------------------------------------------------------------------

    fn node(
        parent: Option<usize>,
        prev: Option<usize>,
        next: Option<usize>,
        last_child: Option<usize>,
    ) -> Node {
        Node {
            parent: parent.map(NodeId::new),
            previous_sibling: prev.map(NodeId::new),
            next_sibling: next.map(NodeId::new),
            last_child: last_child.map(NodeId::new),
        }
    }

    fn items(nodes: &[Node]) -> Vec<NodeHierarchyItem> {
        nodes.iter().map(|n| NodeHierarchyItem::from(*n)).collect()
    }

    fn div_with(id: Option<&str>, class: Option<&str>) -> NodeData {
        let mut nd = NodeData::create_div();
        if let Some(i) = id {
            nd.add_id(i.into());
        }
        if let Some(c) = class {
            nd.add_class(c.into());
        }
        nd
    }

    fn node_with_attrs(attrs: Vec<AttributeType>) -> NodeData {
        let mut nd = NodeData::create_div();
        nd.set_attributes(attrs.into());
        nd
    }

    fn custom(name: &str, value: &str) -> AttributeType {
        AttributeType::Custom(AttributeNameValue {
            attr_name: name.into(),
            value: value.into(),
        })
    }

    fn attr_sel(name: &str, op: AttributeMatchOp, value: Option<&str>) -> CssAttributeSelector {
        CssAttributeSelector {
            name: name.into(),
            op,
            value: value.map_or(OptionString::None, |v| OptionString::Some(v.into())),
        }
    }

    fn info(index_in_parent: u32, is_last_child: bool) -> CascadeInfo {
        CascadeInfo {
            index_in_parent,
            is_last_child,
        }
    }

    /// The shared fixture DOM. Note node 2 is a **text node** sitting between
    /// two element siblings — the whole point is to exercise the "text nodes
    /// are not counted as element siblings" rule (CSS Selectors L4 §13).
    ///
    /// ```text
    /// 0 body
    /// ├── 1 div#first.a
    /// ├── 2 "hello"          (text)
    /// ├── 3 div.b
    /// │   └── 4 p.inner
    /// └── 5 div.c
    /// ```
    fn sample_hierarchy() -> Vec<Node> {
        vec![
            node(None, None, None, Some(5)),
            node(Some(0), None, Some(2), None),
            node(Some(0), Some(1), Some(3), None),
            node(Some(0), Some(2), Some(5), Some(4)),
            node(Some(3), None, None, None),
            node(Some(0), Some(3), None, None),
        ]
    }

    fn sample_node_data() -> Vec<NodeData> {
        let mut p = NodeData::create_node(NodeType::P);
        p.add_class("inner".into());
        vec![
            NodeData::create_body(),
            div_with(Some("first"), Some("a")),
            NodeData::create_text_do_not_use_without_block_level_wrapper("hello"),
            div_with(None, Some("b")),
            p,
            div_with(None, Some("c")),
        ]
    }

    /// Runs `matches_html_element` against the sample fixture.
    fn matches(
        selectors: Vec<CssPathSelector>,
        node_index: usize,
        expected_path_ending: Option<CssPathPseudoSelector>,
    ) -> bool {
        let hierarchy = sample_hierarchy();
        let hier_items = items(&hierarchy);
        let data = sample_node_data();

        let hierarchy_ref = NodeHierarchyRef::from_slice(&hierarchy);
        let data_ref = NodeDataContainerRef::from_slice(&data);
        let depths = hierarchy_ref.get_parents_sorted_by_depth();
        let cascade = construct_html_cascade_tree(&hierarchy_ref, &depths, &data_ref);

        matches_html_element(
            &CssPath::new(selectors),
            NodeId::new(node_index),
            &NodeDataContainerRef::from_slice(&hier_items),
            &data_ref,
            &cascade.as_ref(),
            expected_path_ending,
        )
    }

    // ---------------------------------------------------------------------
    // CascadeInfoVec::as_container  (getter)
    // ---------------------------------------------------------------------

    #[test]
    fn cascade_info_vec_as_container_preserves_every_entry() {
        let v: CascadeInfoVec = vec![info(0, false), info(1, false), info(2, true)].into();
        let c = v.as_container();

        assert_eq!(c.len(), 3);
        assert!(!c.is_empty());
        assert_eq!(c[NodeId::new(0)], info(0, false));
        assert_eq!(c[NodeId::new(2)], info(2, true));
        // Out-of-range access must be a `None`, not a panic.
        assert_eq!(c.get(NodeId::new(3)), None);
        assert_eq!(c.get(NodeId::new(usize::MAX)), None);
    }

    #[test]
    fn cascade_info_vec_as_container_on_empty_vec_does_not_panic() {
        let v: CascadeInfoVec = Vec::new().into();
        let c = v.as_container();
        assert_eq!(c.len(), 0);
        assert!(c.is_empty());
        assert_eq!(c.get(NodeId::new(0)), None);
    }

    #[test]
    fn cascade_info_vec_as_container_survives_extreme_field_values() {
        let v: CascadeInfoVec = vec![info(u32::MAX, true)].into();
        let c = v.as_container();
        assert_eq!(c[NodeId::new(0)].index_in_parent, u32::MAX);
        assert!(c[NodeId::new(0)].is_last_child);
    }

    // ---------------------------------------------------------------------
    // CssGroupIterator
    // ---------------------------------------------------------------------

    #[test]
    fn css_group_iterator_new_records_the_path_and_yields_nothing_when_empty() {
        let empty: [CssPathSelector; 0] = [];
        let it = CssGroupIterator::new(&empty);
        assert!(it.css_path.is_empty());
        assert_eq!(CssGroupIterator::new(&empty).count(), 0);
    }

    #[test]
    fn css_group_iterator_new_keeps_the_slice_it_was_given() {
        let path = vec![
            CssPathSelector::Global,
            CssPathSelector::Children,
            CssPathSelector::Class("x".into()),
        ];
        let it = CssGroupIterator::new(&path);
        assert_eq!(it.css_path.len(), 3);
        assert_eq!(it.css_path, path.as_slice());
    }

    #[test]
    fn css_group_iterator_splits_right_to_left_with_the_left_hand_combinator() {
        // `body > .foo.main .baz`
        let path = vec![
            CssPathSelector::Type(NodeTypeTag::Body),
            CssPathSelector::DirectChildren,
            CssPathSelector::Class("foo".into()),
            CssPathSelector::Class("main".into()),
            CssPathSelector::Children,
            CssPathSelector::Class("baz".into()),
        ];

        let groups: Vec<_> = CssGroupIterator::new(&path).collect();
        assert_eq!(groups.len(), 3);

        // Group 0 is the RIGHTMOST group (the subject of the selector).
        assert_eq!(groups[0].0, vec![&path[5]]);
        assert_eq!(groups[0].1, CssGroupSplitReason::Children);

        assert_eq!(groups[1].0, vec![&path[2], &path[3]]);
        assert_eq!(groups[1].1, CssGroupSplitReason::DirectChildren);

        assert_eq!(groups[2].0, vec![&path[0]]);
        // NOTE: the leftmost group's `reason` is a carry-over from the previous
        // split (there is no combinator to its left). `matches_html_element`
        // never reads it — it uses `groups[i - 1].1` as the combinator for
        // group `i` — so we deliberately do not assert on it here.
    }

    #[test]
    fn css_group_iterator_yields_an_empty_group_for_a_trailing_combinator() {
        // Malformed path `.foo >` (a combinator with nothing to its right).
        let path = vec![
            CssPathSelector::Class("foo".into()),
            CssPathSelector::DirectChildren,
        ];
        let groups: Vec<_> = CssGroupIterator::new(&path).collect();

        assert_eq!(groups.len(), 2);
        assert!(
            groups[0].0.is_empty(),
            "the group to the right of the dangling combinator is empty"
        );
        assert_eq!(groups[0].1, CssGroupSplitReason::DirectChildren);
        assert_eq!(groups[1].0, vec![&path[0]]);
    }

    #[test]
    fn css_group_iterator_terminates_on_consecutive_combinators() {
        // `.a > ~ .b` — two combinators in a row (parser should never emit this,
        // but the iterator must not loop forever or drop selectors).
        let path = vec![
            CssPathSelector::Class("a".into()),
            CssPathSelector::Children,
            CssPathSelector::DirectChildren,
            CssPathSelector::Class("b".into()),
        ];
        let groups: Vec<_> = CssGroupIterator::new(&path).collect();

        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].0, vec![&path[3]]);
        assert!(
            groups[1].0.is_empty(),
            "the group between the two combinators is empty"
        );
        assert_eq!(groups[2].0, vec![&path[0]]);
    }

    #[test]
    fn css_group_iterator_only_combinators_yields_only_empty_groups() {
        let path = vec![
            CssPathSelector::Children,
            CssPathSelector::DirectChildren,
            CssPathSelector::AdjacentSibling,
            CssPathSelector::GeneralSibling,
        ];
        let groups: Vec<_> = CssGroupIterator::new(&path).collect();

        // Four combinators, four splits, and the final `new_idx == 0 &&
        // current_path.is_empty()` branch ends the iteration.
        assert_eq!(groups.len(), 4);
        assert!(groups.iter().all(|(g, _)| g.is_empty()));
    }

    #[test]
    fn css_group_iterator_conserves_every_non_combinator_selector_on_a_huge_path() {
        // 10_000 selectors: `.c0 .c1 .c2 ...` — must terminate and must not
        // lose or duplicate a single selector.
        let mut path = Vec::new();
        for i in 0..5_000u32 {
            if i != 0 {
                path.push(CssPathSelector::Children);
            }
            path.push(CssPathSelector::Class(alloc::format!(".c{i}").into()));
        }

        let groups: Vec<_> = CssGroupIterator::new(&path).collect();
        assert_eq!(groups.len(), 5_000);
        let total: usize = groups.iter().map(|(g, _)| g.len()).sum();
        assert_eq!(
            total, 5_000,
            "every non-combinator selector must appear in exactly one group"
        );
        assert!(groups
            .iter()
            .all(|(_, r)| *r == CssGroupSplitReason::Children));
    }

    // ---------------------------------------------------------------------
    // rule_ends_with
    // ---------------------------------------------------------------------

    #[test]
    fn rule_ends_with_empty_path_is_always_false() {
        let empty = CssPath::new(Vec::new());
        assert!(!rule_ends_with(&empty, None));
        assert!(!rule_ends_with(&empty, Some(CssPathPseudoSelector::Hover)));
        assert!(!rule_ends_with(
            &empty,
            Some(CssPathPseudoSelector::NthChild(CssNthChildSelector::Even))
        ));
    }

    #[test]
    fn rule_ends_with_none_rejects_interactive_pseudos_but_keeps_structural_ones() {
        let ends_with = |s: CssPathSelector| {
            rule_ends_with(
                &CssPath::new(vec![CssPathSelector::Class("a".into()), s]),
                None,
            )
        };

        // Interactive => rejected for the "normal" (non-pseudo) pass.
        for p in [
            CssPathPseudoSelector::Hover,
            CssPathPseudoSelector::Active,
            CssPathPseudoSelector::Focus,
            CssPathPseudoSelector::Backdrop,
            CssPathPseudoSelector::Dragging,
            CssPathPseudoSelector::DragOver,
        ] {
            assert!(
                !ends_with(CssPathSelector::PseudoSelector(p.clone())),
                "interactive pseudo {p:?} must not end a `None` rule"
            );
        }

        // Structural => kept.
        assert!(ends_with(CssPathSelector::PseudoSelector(
            CssPathPseudoSelector::First
        )));
        assert!(ends_with(CssPathSelector::PseudoSelector(
            CssPathPseudoSelector::Last
        )));
        assert!(ends_with(CssPathSelector::PseudoSelector(
            CssPathPseudoSelector::NthChild(CssNthChildSelector::Odd)
        )));
        // Non-pseudo endings are always kept.
        assert!(ends_with(CssPathSelector::Global));
        assert!(ends_with(CssPathSelector::Class("b".into())));
        assert!(ends_with(CssPathSelector::Type(NodeTypeTag::Div)));
        assert!(ends_with(CssPathSelector::DirectChildren));
    }

    #[test]
    fn rule_ends_with_some_target_requires_an_exact_match_on_the_last_selector() {
        let hover = CssPath::new(vec![
            CssPathSelector::Class("a".into()),
            CssPathSelector::PseudoSelector(CssPathPseudoSelector::Hover),
        ]);
        assert!(rule_ends_with(&hover, Some(CssPathPseudoSelector::Hover)));
        assert!(!rule_ends_with(&hover, Some(CssPathPseudoSelector::Active)));
        assert!(!rule_ends_with(&hover, Some(CssPathPseudoSelector::Focus)));

        // A non-pseudo ending never matches a pseudo target.
        let plain = CssPath::new(vec![CssPathSelector::Class("a".into())]);
        assert!(!rule_ends_with(&plain, Some(CssPathPseudoSelector::Hover)));

        // Documented limitation: only the VERY LAST selector is inspected, so
        // `.a:hover:first` does not count as ending with `:hover`.
        let compound = CssPath::new(vec![
            CssPathSelector::Class("a".into()),
            CssPathSelector::PseudoSelector(CssPathPseudoSelector::Hover),
            CssPathSelector::PseudoSelector(CssPathPseudoSelector::First),
        ]);
        assert!(!rule_ends_with(
            &compound,
            Some(CssPathPseudoSelector::Hover)
        ));
        assert!(rule_ends_with(
            &compound,
            Some(CssPathPseudoSelector::First)
        ));
    }

    #[test]
    fn rule_ends_with_compares_nth_child_and_lang_payloads() {
        let nth = |s| {
            CssPath::new(vec![CssPathSelector::PseudoSelector(
                CssPathPseudoSelector::NthChild(s),
            )])
        };
        assert!(rule_ends_with(
            &nth(CssNthChildSelector::Number(3)),
            Some(CssPathPseudoSelector::NthChild(
                CssNthChildSelector::Number(3)
            ))
        ));
        assert!(!rule_ends_with(
            &nth(CssNthChildSelector::Number(3)),
            Some(CssPathPseudoSelector::NthChild(
                CssNthChildSelector::Number(4)
            ))
        ));
        assert!(!rule_ends_with(
            &nth(CssNthChildSelector::Even),
            Some(CssPathPseudoSelector::NthChild(CssNthChildSelector::Odd))
        ));

        // Unicode / boundary language tags must compare by value, not by prefix.
        let lang = |s: &str| {
            CssPath::new(vec![CssPathSelector::PseudoSelector(
                CssPathPseudoSelector::Lang(s.into()),
            )])
        };
        assert!(rule_ends_with(
            &lang("zh-Hant-🎉"),
            Some(CssPathPseudoSelector::Lang("zh-Hant-🎉".into()))
        ));
        assert!(!rule_ends_with(
            &lang("zh-Hant-🎉"),
            Some(CssPathPseudoSelector::Lang("zh".into()))
        ));
        assert!(!rule_ends_with(
            &lang(""),
            Some(CssPathPseudoSelector::Lang("de".into()))
        ));
    }

    // ---------------------------------------------------------------------
    // match_first_child / match_last_child
    // ---------------------------------------------------------------------

    #[test]
    fn match_first_and_last_child_read_only_their_own_field() {
        assert!(match_first_child(info(0, false)));
        assert!(!match_first_child(info(1, true)));
        assert!(!match_first_child(info(u32::MAX, true)));

        assert!(match_last_child(info(0, true)));
        assert!(!match_last_child(info(0, false)));
        assert!(match_last_child(info(u32::MAX, true)));
    }

    // ---------------------------------------------------------------------
    // match_nth_child  (numeric: 1-indexing, saturation, div-by-zero, overflow)
    // ---------------------------------------------------------------------

    #[test]
    fn match_nth_child_is_one_indexed() {
        // index_in_parent 0 == :nth-child(1) == odd
        assert!(match_nth_child(
            info(0, false),
            &CssNthChildSelector::Number(1)
        ));
        assert!(!match_nth_child(
            info(0, false),
            &CssNthChildSelector::Number(0)
        ));
        assert!(match_nth_child(info(0, false), &CssNthChildSelector::Odd));
        assert!(!match_nth_child(info(0, false), &CssNthChildSelector::Even));

        assert!(match_nth_child(
            info(1, false),
            &CssNthChildSelector::Number(2)
        ));
        assert!(match_nth_child(info(1, false), &CssNthChildSelector::Even));
        assert!(!match_nth_child(info(1, false), &CssNthChildSelector::Odd));
    }

    #[test]
    fn match_nth_child_number_matches_exactly_one_index() {
        for n in 1..64u32 {
            for idx in 0..64u32 {
                let matched = match_nth_child(info(idx, false), &CssNthChildSelector::Number(n));
                assert_eq!(matched, idx + 1 == n, "nth-child({n}) vs index {idx}");
            }
        }
        // `:nth-child(0)` can never match: the index is 1-based.
        for idx in 0..64u32 {
            assert!(!match_nth_child(
                info(idx, false),
                &CssNthChildSelector::Number(0)
            ));
        }
    }

    #[test]
    fn match_nth_child_even_and_odd_partition_every_index() {
        for idx in 0..1_000u32 {
            let even = match_nth_child(info(idx, false), &CssNthChildSelector::Even);
            let odd = match_nth_child(info(idx, false), &CssNthChildSelector::Odd);
            assert_ne!(even, odd, "index {idx} must be exactly one of even/odd");
        }
    }

    #[test]
    fn match_nth_child_pattern_agrees_with_the_even_and_odd_shorthands() {
        // CSS: `even` == `2n`, `odd` == `2n+1`.
        for idx in 0..256u32 {
            let even_pat = CssNthChildSelector::Pattern(CssNthChildPattern {
                pattern_repeat: 2,
                offset: 0,
            });
            let odd_pat = CssNthChildSelector::Pattern(CssNthChildPattern {
                pattern_repeat: 2,
                offset: 1,
            });
            assert_eq!(
                match_nth_child(info(idx, false), &even_pat),
                match_nth_child(info(idx, false), &CssNthChildSelector::Even),
                "2n disagrees with `even` at index {idx}"
            );
            assert_eq!(
                match_nth_child(info(idx, false), &odd_pat),
                match_nth_child(info(idx, false), &CssNthChildSelector::Odd),
                "2n+1 disagrees with `odd` at index {idx}"
            );
        }
    }

    #[test]
    fn match_nth_child_zero_repeat_never_divides_by_zero() {
        // `0n+3` matches only the 3rd child; `0n+0` matches nothing (1-based).
        let only_third = CssNthChildSelector::Pattern(CssNthChildPattern {
            pattern_repeat: 0,
            offset: 3,
        });
        let never = CssNthChildSelector::Pattern(CssNthChildPattern {
            pattern_repeat: 0,
            offset: 0,
        });

        for idx in 0..32u32 {
            assert_eq!(match_nth_child(info(idx, false), &only_third), idx == 2);
            assert!(
                !match_nth_child(info(idx, false), &never),
                "0n+0 must never match (index is 1-based)"
            );
        }
    }

    #[test]
    fn match_nth_child_pattern_below_the_offset_does_not_underflow() {
        // `2n+5`: nothing below the 5th child may match, and the `index - offset`
        // subtraction must never be reached for those.
        let pat = CssNthChildSelector::Pattern(CssNthChildPattern {
            pattern_repeat: 2,
            offset: 5,
        });
        for idx in 0..4u32 {
            assert!(
                !match_nth_child(info(idx, false), &pat),
                "index {idx} < offset"
            );
        }
        assert!(match_nth_child(info(4, false), &pat)); // 5th child
        assert!(!match_nth_child(info(5, false), &pat)); // 6th
        assert!(match_nth_child(info(6, false), &pat)); // 7th
    }

    #[test]
    fn match_nth_child_with_a_u32_max_offset_does_not_underflow() {
        let pat = CssNthChildSelector::Pattern(CssNthChildPattern {
            pattern_repeat: 1,
            offset: u32::MAX,
        });
        assert!(!match_nth_child(info(0, false), &pat));
        assert!(!match_nth_child(info(1_000, false), &pat));
        // index == u32::MAX (index_in_parent == u32::MAX - 1) is the largest
        // index that can be formed without overflowing the `+ 1`.
        assert!(match_nth_child(info(u32::MAX - 1, false), &pat));
    }

    /// BOUNDARY: `match_nth_child` computes `index_in_parent + 1` unchecked.
    /// `index_in_parent == u32::MAX` is reachable — `construct_html_cascade_tree`
    /// itself saturates to `u32::MAX` (`unwrap_or(u32::MAX)`), and `CascadeInfo`
    /// is a `#[repr(C)]` struct with public fields. Debug builds therefore panic
    /// on overflow, release builds wrap the index to 0. Assert that the wrap can
    /// never silently produce the *wrong* answer for even/odd/number.
    #[cfg(feature = "std")]
    #[test]
    fn match_nth_child_at_u32_max_index_never_answers_wrongly() {
        // The true 1-based index here is 2^32, which is even.
        let even = std::panic::catch_unwind(|| {
            match_nth_child(info(u32::MAX, false), &CssNthChildSelector::Even)
        });
        let odd = std::panic::catch_unwind(|| {
            match_nth_child(info(u32::MAX, false), &CssNthChildSelector::Odd)
        });
        let one = std::panic::catch_unwind(|| {
            match_nth_child(info(u32::MAX, false), &CssNthChildSelector::Number(1))
        });

        // debug: the overflow check fires — loud failure, acceptable.
        // release: the index wraps to 0, which must still not flip an answer.
        if let Ok(v) = even {
            assert!(v, "2^32 is even");
        }
        if let Ok(v) = odd {
            assert!(!v, "2^32 is not odd");
        }
        if let Ok(v) = one {
            assert!(!v, "the 2^32-th child is not the 1st child");
        }
    }

    // ---------------------------------------------------------------------
    // match_interactive_pseudo / match_pseudo_selector
    // ---------------------------------------------------------------------

    #[test]
    fn match_interactive_pseudo_needs_both_the_last_group_and_the_expected_ending() {
        let hover = CssPathPseudoSelector::Hover;
        let active = CssPathPseudoSelector::Active;

        assert!(match_interactive_pseudo(&hover, Some(&hover), true));
        assert!(!match_interactive_pseudo(&hover, Some(&hover), false));
        assert!(!match_interactive_pseudo(&hover, Some(&active), true));
        assert!(!match_interactive_pseudo(&hover, None, true));
        assert!(!match_interactive_pseudo(&hover, None, false));
    }

    #[test]
    fn match_pseudo_selector_routes_every_interactive_pseudo_through_the_gate() {
        for p in [
            CssPathPseudoSelector::Hover,
            CssPathPseudoSelector::Active,
            CssPathPseudoSelector::Focus,
            CssPathPseudoSelector::Backdrop,
            CssPathPseudoSelector::Dragging,
            CssPathPseudoSelector::DragOver,
        ] {
            assert!(
                match_pseudo_selector(&p, info(0, false), Some(&p), true),
                "{p:?} must match when it is the expected ending of the last group"
            );
            assert!(
                !match_pseudo_selector(&p, info(0, false), Some(&p), false),
                "{p:?} must not match outside the last content group"
            );
            assert!(
                !match_pseudo_selector(&p, info(0, false), None, true),
                "{p:?} must not match when no pseudo state is expected"
            );
        }
    }

    #[test]
    fn match_pseudo_selector_structural_pseudos_ignore_the_expected_ending() {
        let hover = CssPathPseudoSelector::Hover;

        // :first / :last / :nth-child depend only on the CascadeInfo.
        for (expected, is_last) in [(None, true), (Some(&hover), false), (Some(&hover), true)] {
            assert!(match_pseudo_selector(
                &CssPathPseudoSelector::First,
                info(0, false),
                expected,
                is_last
            ));
            assert!(!match_pseudo_selector(
                &CssPathPseudoSelector::First,
                info(1, false),
                expected,
                is_last
            ));
            assert!(match_pseudo_selector(
                &CssPathPseudoSelector::Last,
                info(9, true),
                expected,
                is_last
            ));
            assert!(match_pseudo_selector(
                &CssPathPseudoSelector::NthChild(CssNthChildSelector::Number(10)),
                info(9, true),
                expected,
                is_last
            ));
        }
    }

    #[test]
    fn match_pseudo_selector_lang_only_matches_the_expected_lang() {
        let de = CssPathPseudoSelector::Lang("de".into());
        let en = CssPathPseudoSelector::Lang("en".into());

        assert!(match_pseudo_selector(&de, info(0, false), Some(&de), true));
        assert!(!match_pseudo_selector(&de, info(0, false), Some(&en), true));
        assert!(!match_pseudo_selector(&de, info(0, false), None, true));
        assert!(!match_pseudo_selector(
            &de,
            info(0, false),
            Some(&CssPathPseudoSelector::Hover),
            true
        ));

        // Unicode + empty language tags must not panic and must compare by value.
        let emoji = CssPathPseudoSelector::Lang("de-🎉".into());
        assert!(match_pseudo_selector(
            &emoji,
            info(0, false),
            Some(&emoji),
            true
        ));
        assert!(!match_pseudo_selector(
            &emoji,
            info(0, false),
            Some(&de),
            true
        ));
        let empty = CssPathPseudoSelector::Lang("".into());
        assert!(match_pseudo_selector(
            &empty,
            info(0, false),
            Some(&empty),
            true
        ));
    }

    // ---------------------------------------------------------------------
    // match_attribute_selector
    // ---------------------------------------------------------------------

    #[test]
    fn match_attribute_selector_on_a_node_without_attributes_is_always_false() {
        let nd = NodeData::create_div();
        for op in [
            AttributeMatchOp::Exists,
            AttributeMatchOp::Eq,
            AttributeMatchOp::Includes,
            AttributeMatchOp::DashMatch,
            AttributeMatchOp::Prefix,
            AttributeMatchOp::Suffix,
            AttributeMatchOp::Substring,
        ] {
            assert!(!match_attribute_selector(
                &attr_sel("data-x", op, Some("v")),
                &nd
            ));
            assert!(!match_attribute_selector(&attr_sel("", op, None), &nd));
        }
    }

    #[test]
    fn match_attribute_selector_exists_matches_any_value_including_the_empty_one() {
        let nd = node_with_attrs(vec![custom("data-x", "")]);
        assert!(match_attribute_selector(
            &attr_sel("data-x", AttributeMatchOp::Exists, None),
            &nd
        ));
        // Exists ignores the value entirely, even if one is (wrongly) supplied.
        assert!(match_attribute_selector(
            &attr_sel("data-x", AttributeMatchOp::Exists, Some("nonsense")),
            &nd
        ));
        assert!(!match_attribute_selector(
            &attr_sel("data-y", AttributeMatchOp::Exists, None),
            &nd
        ));
    }

    #[test]
    fn match_attribute_selector_operator_without_a_value_never_matches() {
        let nd = node_with_attrs(vec![custom("data-x", "value")]);
        for op in [
            AttributeMatchOp::Eq,
            AttributeMatchOp::Includes,
            AttributeMatchOp::DashMatch,
            AttributeMatchOp::Prefix,
            AttributeMatchOp::Suffix,
            AttributeMatchOp::Substring,
        ] {
            assert!(
                !match_attribute_selector(&attr_sel("data-x", op, None), &nd),
                "{op:?} with a missing target value must be rejected, not matched"
            );
        }
    }

    #[test]
    fn match_attribute_selector_empty_target_never_matches_the_substring_family() {
        // `[x^=""]` / `[x$=""]` / `[x*=""]` / `[x~=""]` would otherwise match
        // every node (every string starts with / contains the empty string).
        let nd = node_with_attrs(vec![custom("data-x", "abc")]);
        for op in [
            AttributeMatchOp::Prefix,
            AttributeMatchOp::Suffix,
            AttributeMatchOp::Substring,
            AttributeMatchOp::Includes,
        ] {
            assert!(
                !match_attribute_selector(&attr_sel("data-x", op, Some("")), &nd),
                "{op:?} with an empty target must not match"
            );
        }
        // Eq is the exception: `[x=""]` legitimately means "the empty value".
        assert!(!match_attribute_selector(
            &attr_sel("data-x", AttributeMatchOp::Eq, Some("")),
            &nd
        ));
        let empty = node_with_attrs(vec![custom("data-x", "")]);
        assert!(match_attribute_selector(
            &attr_sel("data-x", AttributeMatchOp::Eq, Some("")),
            &empty
        ));
    }

    #[test]
    fn match_attribute_selector_includes_matches_one_of_several_class_entries() {
        // Classes are stored as separate `AttributeType::Class` entries, so the
        // matcher has to be "any value satisfies the op" (see the fn doc).
        let mut nd = NodeData::create_div();
        nd.add_class("foo".into());
        nd.add_class("primary".into());
        nd.add_class("bar".into());

        assert!(match_attribute_selector(
            &attr_sel("class", AttributeMatchOp::Includes, Some("primary")),
            &nd
        ));
        assert!(!match_attribute_selector(
            &attr_sel("class", AttributeMatchOp::Includes, Some("prim")),
            &nd
        ));
        // A target containing whitespace is invalid for `~=` and must not match.
        assert!(!match_attribute_selector(
            &attr_sel("class", AttributeMatchOp::Includes, Some("foo bar")),
            &nd
        ));
        assert!(!match_attribute_selector(
            &attr_sel("class", AttributeMatchOp::Includes, Some("\t")),
            &nd
        ));
    }

    #[test]
    fn match_attribute_selector_dash_match_requires_a_dash_boundary() {
        let nd = node_with_attrs(vec![AttributeType::Lang("en-US".into())]);

        assert!(match_attribute_selector(
            &attr_sel("lang", AttributeMatchOp::DashMatch, Some("en")),
            &nd
        ));
        assert!(match_attribute_selector(
            &attr_sel("lang", AttributeMatchOp::DashMatch, Some("en-US")),
            &nd
        ));
        // A prefix that does not end on the `-` boundary must not match.
        assert!(!match_attribute_selector(
            &attr_sel("lang", AttributeMatchOp::DashMatch, Some("en-U")),
            &nd
        ));
        assert!(!match_attribute_selector(
            &attr_sel("lang", AttributeMatchOp::DashMatch, Some("e")),
            &nd
        ));

        // `en` must not match the value `english` (no dash boundary).
        let english = node_with_attrs(vec![AttributeType::Lang("english".into())]);
        assert!(!match_attribute_selector(
            &attr_sel("lang", AttributeMatchOp::DashMatch, Some("en")),
            &english
        ));
    }

    #[test]
    fn match_attribute_selector_name_matching_is_exact_and_case_sensitive() {
        let nd = node_with_attrs(vec![custom("data-foo", "v")]);
        assert!(match_attribute_selector(
            &attr_sel("data-foo", AttributeMatchOp::Eq, Some("v")),
            &nd
        ));
        assert!(!match_attribute_selector(
            &attr_sel("data-fo", AttributeMatchOp::Eq, Some("v")),
            &nd
        ));
        assert!(!match_attribute_selector(
            &attr_sel("data-foo2", AttributeMatchOp::Eq, Some("v")),
            &nd
        ));
        assert!(!match_attribute_selector(
            &attr_sel("DATA-FOO", AttributeMatchOp::Eq, Some("v")),
            &nd
        ));
        assert!(!match_attribute_selector(
            &attr_sel("", AttributeMatchOp::Exists, None),
            &nd
        ));
    }

    #[test]
    fn match_attribute_selector_handles_unicode_values_on_char_boundaries() {
        // "héllo-🎉-世界" has no ASCII 'e' and no ASCII-splittable emoji.
        let nd = node_with_attrs(vec![custom("data-x", "héllo-🎉-世界")]);

        assert!(match_attribute_selector(
            &attr_sel("data-x", AttributeMatchOp::Prefix, Some("hé")),
            &nd
        ));
        assert!(match_attribute_selector(
            &attr_sel("data-x", AttributeMatchOp::Suffix, Some("世界")),
            &nd
        ));
        assert!(match_attribute_selector(
            &attr_sel("data-x", AttributeMatchOp::Substring, Some("🎉")),
            &nd
        ));
        assert!(match_attribute_selector(
            &attr_sel("data-x", AttributeMatchOp::Eq, Some("héllo-🎉-世界")),
            &nd
        ));
        // No false positive from the ASCII byte inside a multi-byte codepoint.
        assert!(!match_attribute_selector(
            &attr_sel("data-x", AttributeMatchOp::Substring, Some("e")),
            &nd
        ));
        // DashMatch on a unicode segment.
        assert!(match_attribute_selector(
            &attr_sel("data-x", AttributeMatchOp::DashMatch, Some("héllo")),
            &nd
        ));
    }

    #[test]
    fn match_attribute_selector_handles_huge_values() {
        let huge = "ä".repeat(100_000);
        let value = alloc::format!("{huge}-tail");
        let nd = node_with_attrs(vec![custom("data-big", &value)]);

        assert!(match_attribute_selector(
            &attr_sel("data-big", AttributeMatchOp::Suffix, Some("-tail")),
            &nd
        ));
        assert!(match_attribute_selector(
            &attr_sel("data-big", AttributeMatchOp::Prefix, Some("ää")),
            &nd
        ));
        assert!(match_attribute_selector(
            &attr_sel("data-big", AttributeMatchOp::DashMatch, Some(huge.as_str())),
            &nd
        ));
        assert!(!match_attribute_selector(
            &attr_sel("data-big", AttributeMatchOp::Eq, Some(huge.as_str())),
            &nd
        ));
    }

    // ---------------------------------------------------------------------
    // match_single_selector / selector_group_matches
    // ---------------------------------------------------------------------

    #[test]
    fn match_single_selector_global_matches_elements_but_never_text_nodes() {
        let div = NodeData::create_div();
        let text = NodeData::create_text_do_not_use_without_block_level_wrapper("hello");
        assert!(match_single_selector(
            &CssPathSelector::Global,
            info(0, false),
            &div,
            NodeId::new(0),
            None,
            true,
            false
        ));
        // Per CSS, `*` matches ELEMENTS only. Text nodes take styling by
        // inheritance; a universal rule hitting them directly overwrote
        // freshly inherited values (`* { color }` vs `p { color }`).
        assert!(!match_single_selector(
            &CssPathSelector::Global,
            info(u32::MAX, true),
            &text,
            NodeId::new(usize::MAX),
            None,
            false,
            false
        ));
    }

    #[test]
    fn global_matches_a_text_node_only_when_the_rule_is_scoped_to_exactly_it() {
        // Inline-style semantics (miniword ENGINE-ISSUE 4):
        // `create_text_do_not_use_without_block_level_wrapper("x").with_css("color: white")` produces
        // `[Root(n..=n), Global]` — the author addressed THIS node, so the
        // universal selector must match despite it being a text node.
        let text = NodeData::create_text_do_not_use_without_block_level_wrapper("hello");
        let nid = NodeId::new(7);
        let self_scope = CssPathSelector::Root(CssScopeRange { start: 7, end: 7 });
        let global = CssPathSelector::Global;

        let group: Vec<&CssPathSelector> = vec![&self_scope, &global];
        assert!(
            selector_group_matches(&group, info(0, true), &text, nid, None, true),
            "a node-only-scoped bare-decl rule must style its own text node"
        );

        // Negative control 1: the SAME group on a DIFFERENT text node (the
        // scope range excludes it) must not match.
        assert!(
            !selector_group_matches(&group, info(0, true), &text, NodeId::new(8), None, true),
            "node-only scope must not leak to other text nodes"
        );

        // Negative control 2: a SUBTREE-scoped `* { }` (stylesheet semantics,
        // start != end) keeps refusing text nodes — the historical
        // `* { color: #666 }` inheritance-overwrite bug must stay fixed.
        let subtree_scope = CssPathSelector::Root(CssScopeRange { start: 0, end: 20 });
        let group2: Vec<&CssPathSelector> = vec![&subtree_scope, &global];
        assert!(
            !selector_group_matches(&group2, info(0, true), &text, nid, None, true),
            "subtree-scoped universal selectors must never match text nodes"
        );

        // Negative control 3: an UNSCOPED bare `*` refuses text nodes too.
        let group3: Vec<&CssPathSelector> = vec![&global];
        assert!(!selector_group_matches(
            &group3,
            info(0, true),
            &text,
            nid,
            None,
            true
        ));
    }

    #[test]
    fn match_single_selector_never_matches_a_combinator() {
        // Combinators must be split out by the group iterator; if one ever
        // reaches the matcher it must fail closed, not match everything.
        let div = NodeData::create_div();
        for c in [
            CssPathSelector::DirectChildren,
            CssPathSelector::Children,
            CssPathSelector::AdjacentSibling,
            CssPathSelector::GeneralSibling,
        ] {
            assert!(!match_single_selector(
                &c,
                info(0, true),
                &div,
                NodeId::new(0),
                None,
                true,
                false
            ));
        }
    }

    #[test]
    fn match_single_selector_root_scope_range_is_inclusive_on_both_ends() {
        let div = NodeData::create_div();
        let sel = CssPathSelector::Root(CssScopeRange { start: 2, end: 4 });

        for id in [2usize, 3, 4] {
            assert!(match_single_selector(
                &sel,
                info(0, false),
                &div,
                NodeId::new(id),
                None,
                true,
                false
            ));
        }
        for id in [0usize, 1, 5, usize::MAX] {
            assert!(!match_single_selector(
                &sel,
                info(0, false),
                &div,
                NodeId::new(id),
                None,
                true,
                false
            ));
        }
    }

    #[test]
    fn match_single_selector_root_scope_with_an_inverted_or_full_range() {
        let div = NodeData::create_div();

        // Inverted range (start > end) matches nothing, and must not panic.
        let inverted = CssPathSelector::Root(CssScopeRange { start: 9, end: 2 });
        for id in [0usize, 2, 9, usize::MAX] {
            assert!(!match_single_selector(
                &inverted,
                info(0, false),
                &div,
                NodeId::new(id),
                None,
                true,
                false
            ));
        }

        // Full range matches every node id, including usize::MAX.
        let full = CssPathSelector::Root(CssScopeRange {
            start: 0,
            end: usize::MAX,
        });
        for id in [0usize, 1, usize::MAX] {
            assert!(match_single_selector(
                &full,
                info(0, false),
                &div,
                NodeId::new(id),
                None,
                true,
                false
            ));
        }
    }

    #[test]
    fn match_single_selector_type_class_and_id() {
        let mut nd = div_with(Some("first"), Some("a"));
        nd.add_class("日本語-🎉".into());

        let hit = |s: &CssPathSelector| {
            match_single_selector(s, info(0, false), &nd, NodeId::new(0), None, true, false)
        };

        assert!(hit(&CssPathSelector::Type(NodeTypeTag::Div)));
        assert!(!hit(&CssPathSelector::Type(NodeTypeTag::P)));
        assert!(hit(&CssPathSelector::Class("a".into())));
        assert!(hit(&CssPathSelector::Class("日本語-🎉".into())));
        assert!(
            !hit(&CssPathSelector::Class("日本語".into())),
            "no prefix matching"
        );
        assert!(!hit(&CssPathSelector::Class("".into())));
        assert!(hit(&CssPathSelector::Id("first".into())));
        assert!(!hit(&CssPathSelector::Id("firs".into())));
        // Ids and classes must not cross over.
        assert!(!hit(&CssPathSelector::Class("first".into())));
        assert!(!hit(&CssPathSelector::Id("a".into())));
    }

    #[test]
    fn selector_group_matches_requires_every_selector_in_the_group() {
        let nd = div_with(Some("first"), Some("a"));
        let div = CssPathSelector::Type(NodeTypeTag::Div);
        let class_a = CssPathSelector::Class("a".into());
        let class_z = CssPathSelector::Class("zzz".into());

        let group: Vec<&CssPathSelector> = vec![&div, &class_a];
        assert!(selector_group_matches(
            &group,
            info(0, false),
            &nd,
            NodeId::new(0),
            None,
            true
        ));

        let group: Vec<&CssPathSelector> = vec![&div, &class_a, &class_z];
        assert!(!selector_group_matches(
            &group,
            info(0, false),
            &nd,
            NodeId::new(0),
            None,
            true
        ));
    }

    #[test]
    fn selector_group_matches_empty_group_matches_vacuously() {
        // This is what a dangling combinator (`.foo >`) produces — see
        // `css_group_iterator_yields_an_empty_group_for_a_trailing_combinator`.
        // `all()` over an empty group is `true`, so such a group matches ANY node.
        let empty: Vec<&CssPathSelector> = Vec::new();
        assert!(selector_group_matches(
            &empty,
            info(0, false),
            &NodeData::create_div(),
            NodeId::new(0),
            None,
            true
        ));
    }

    // ---------------------------------------------------------------------
    // find_non_anonymous_parent / find_non_anonymous_prev_sibling
    // ---------------------------------------------------------------------

    /// ```text
    /// 0 body
    /// ├── 1 <anonymous>
    /// │   └── 2 <anonymous>
    /// │       └── 3 div        <- parent chain must skip 1 and 2
    /// ├── 4 <anonymous>
    /// └── 5 div                <- prev-sibling chain must skip 4 and 1
    /// ```
    fn anonymous_fixture() -> (Vec<Node>, Vec<NodeData>) {
        let hierarchy = vec![
            node(None, None, None, Some(5)),
            node(Some(0), None, Some(4), Some(2)),
            node(Some(1), None, None, Some(3)),
            node(Some(2), None, None, None),
            node(Some(0), Some(1), Some(5), None),
            node(Some(0), Some(4), None, None),
        ];
        let mut anon1 = NodeData::create_div();
        anon1.set_anonymous(true);
        let mut anon2 = NodeData::create_div();
        anon2.set_anonymous(true);
        let mut anon4 = NodeData::create_div();
        anon4.set_anonymous(true);
        let data = vec![
            NodeData::create_body(),
            anon1,
            anon2,
            div_with(None, Some("deep")),
            anon4,
            div_with(None, Some("c")),
        ];
        (hierarchy, data)
    }

    #[test]
    fn find_non_anonymous_parent_skips_a_chain_of_anonymous_boxes() {
        let (hierarchy, data) = anonymous_fixture();
        let hier_items = items(&hierarchy);
        let h = NodeDataContainerRef::from_slice(&hier_items);
        let d = NodeDataContainerRef::from_slice(&data);

        // node 3's real parent is the body (0), not the anonymous 2 / 1.
        assert_eq!(
            find_non_anonymous_parent(NodeId::new(3), &h, &d),
            Some(NodeId::new(0))
        );
        // node 5's parent is the body directly.
        assert_eq!(
            find_non_anonymous_parent(NodeId::new(5), &h, &d),
            Some(NodeId::new(0))
        );
        // the root has no parent at all.
        assert_eq!(find_non_anonymous_parent(NodeId::new(0), &h, &d), None);
    }

    #[test]
    fn find_non_anonymous_parent_returns_none_when_every_ancestor_is_anonymous() {
        // 0 <anonymous root> -> 1 div
        let hierarchy = vec![
            node(None, None, None, Some(1)),
            node(Some(0), None, None, None),
        ];
        let mut anon_root = NodeData::create_div();
        anon_root.set_anonymous(true);
        let data = vec![anon_root, NodeData::create_div()];
        let hier_items = items(&hierarchy);
        let h = NodeDataContainerRef::from_slice(&hier_items);
        let d = NodeDataContainerRef::from_slice(&data);

        assert_eq!(find_non_anonymous_parent(NodeId::new(1), &h, &d), None);
    }

    #[test]
    fn find_non_anonymous_prev_sibling_skips_anonymous_siblings() {
        let (hierarchy, data) = anonymous_fixture();
        let hier_items = items(&hierarchy);
        let h = NodeDataContainerRef::from_slice(&hier_items);
        let d = NodeDataContainerRef::from_slice(&data);

        // node 5's previous siblings are 4 (anonymous) and 1 (anonymous), so
        // there is no non-anonymous previous sibling.
        assert_eq!(
            find_non_anonymous_prev_sibling(NodeId::new(5), &h, &d),
            None
        );
        // a first child has no previous sibling.
        assert_eq!(
            find_non_anonymous_prev_sibling(NodeId::new(1), &h, &d),
            None
        );
        assert_eq!(
            find_non_anonymous_prev_sibling(NodeId::new(0), &h, &d),
            None
        );
    }

    #[test]
    fn find_non_anonymous_prev_sibling_returns_the_nearest_real_sibling() {
        let hierarchy = sample_hierarchy();
        let data = sample_node_data();
        let hier_items = items(&hierarchy);
        let h = NodeDataContainerRef::from_slice(&hier_items);
        let d = NodeDataContainerRef::from_slice(&data);

        // The text node (2) between div.a (1) and div.b (3) is SKIPPED — sibling
        // combinators target elements only — so the previous element sibling of node 3
        // is div.a (1), not the text node. See
        // `matches_html_element_adjacent_sibling_skips_text_nodes`.
        assert_eq!(
            find_non_anonymous_prev_sibling(NodeId::new(3), &h, &d),
            Some(NodeId::new(1))
        );
        assert_eq!(
            find_non_anonymous_prev_sibling(NodeId::new(5), &h, &d),
            Some(NodeId::new(3))
        );
    }

    // ---------------------------------------------------------------------
    // construct_html_cascade_tree
    // ---------------------------------------------------------------------

    #[test]
    fn construct_html_cascade_tree_on_an_empty_hierarchy_is_empty() {
        let hierarchy: Vec<Node> = Vec::new();
        let data: Vec<NodeData> = Vec::new();
        let out = construct_html_cascade_tree(
            &NodeHierarchyRef::from_slice(&hierarchy),
            &[],
            &NodeDataContainerRef::from_slice(&data),
        );
        assert_eq!(out.len(), 0);
        assert!(out.is_empty());
    }

    #[test]
    fn construct_html_cascade_tree_with_no_parents_defaults_every_node() {
        // A single childless root is a LEAF, so `get_parents_sorted_by_depth`
        // returns nothing and every entry keeps the zeroed default.
        let hierarchy = vec![node(None, None, None, None)];
        let data = vec![NodeData::create_body()];
        let hierarchy_ref = NodeHierarchyRef::from_slice(&hierarchy);
        let depths = hierarchy_ref.get_parents_sorted_by_depth();
        assert!(depths.is_empty());

        let out = construct_html_cascade_tree(
            &hierarchy_ref,
            &depths,
            &NodeDataContainerRef::from_slice(&data),
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out.internal[0], CascadeInfo::default());
    }

    #[test]
    fn construct_html_cascade_tree_does_not_count_text_nodes_as_element_siblings() {
        let hierarchy = sample_hierarchy();
        let data = sample_node_data();
        let hierarchy_ref = NodeHierarchyRef::from_slice(&hierarchy);
        let data_ref = NodeDataContainerRef::from_slice(&data);
        let depths = hierarchy_ref.get_parents_sorted_by_depth();

        let out = construct_html_cascade_tree(&hierarchy_ref, &depths, &data_ref);

        assert_eq!(out.len(), hierarchy.len(), "one CascadeInfo per node");
        assert_eq!(out.internal[0], info(0, true), "root");
        assert_eq!(
            out.internal[1],
            info(0, false),
            "div#first.a — 1st element child"
        );
        assert_eq!(
            out.internal[3],
            info(1, false),
            "div.b — 2nd element child (text skipped)"
        );
        assert_eq!(
            out.internal[4],
            info(0, true),
            "p.inner — only child of div.b"
        );
        assert_eq!(
            out.internal[5],
            info(2, true),
            "div.c — 3rd element child and the last one"
        );
        // The text node itself is never the last child and is not an element.
        assert!(!out.internal[2].is_last_child);
    }

    #[test]
    fn construct_html_cascade_tree_ignores_trailing_text_nodes_for_is_last_child() {
        // 0 body -> [1 div, 2 "text", 3 "text"]  => div is still the LAST element child.
        let hierarchy = vec![
            node(None, None, None, Some(3)),
            node(Some(0), None, Some(2), None),
            node(Some(0), Some(1), Some(3), None),
            node(Some(0), Some(2), None, None),
        ];
        let data = vec![
            NodeData::create_body(),
            NodeData::create_div(),
            NodeData::create_text_do_not_use_without_block_level_wrapper("a"),
            NodeData::create_text_do_not_use_without_block_level_wrapper("b"),
        ];
        let hierarchy_ref = NodeHierarchyRef::from_slice(&hierarchy);
        let data_ref = NodeDataContainerRef::from_slice(&data);
        let depths = hierarchy_ref.get_parents_sorted_by_depth();

        let out = construct_html_cascade_tree(&hierarchy_ref, &depths, &data_ref);
        assert_eq!(
            out.internal[1],
            info(0, true),
            "trailing text must not un-last the div"
        );
    }

    #[test]
    fn construct_html_cascade_tree_handles_a_wide_tree() {
        const CHILDREN: usize = 2_000;

        let mut hierarchy = vec![node(None, None, None, Some(CHILDREN))];
        let mut data = vec![NodeData::create_body()];
        for i in 1..=CHILDREN {
            hierarchy.push(node(
                Some(0),
                if i == 1 { None } else { Some(i - 1) },
                if i == CHILDREN { None } else { Some(i + 1) },
                None,
            ));
            data.push(NodeData::create_div());
        }

        let hierarchy_ref = NodeHierarchyRef::from_slice(&hierarchy);
        let data_ref = NodeDataContainerRef::from_slice(&data);
        let depths = hierarchy_ref.get_parents_sorted_by_depth();
        let out = construct_html_cascade_tree(&hierarchy_ref, &depths, &data_ref);

        assert_eq!(out.len(), CHILDREN + 1);
        for i in 1..=CHILDREN {
            let expected = info(u32::try_from(i - 1).unwrap(), i == CHILDREN);
            assert_eq!(out.internal[i], expected, "child {i}");
        }
    }

    #[test]
    fn construct_html_cascade_tree_handles_a_deep_chain() {
        const DEPTH: usize = 1_000;

        let mut hierarchy = Vec::with_capacity(DEPTH);
        let mut data = Vec::with_capacity(DEPTH);
        for i in 0..DEPTH {
            hierarchy.push(node(
                if i == 0 { None } else { Some(i - 1) },
                None,
                None,
                if i + 1 == DEPTH { None } else { Some(i + 1) },
            ));
            data.push(NodeData::create_div());
        }

        let hierarchy_ref = NodeHierarchyRef::from_slice(&hierarchy);
        let data_ref = NodeDataContainerRef::from_slice(&data);
        let depths = hierarchy_ref.get_parents_sorted_by_depth();
        let out = construct_html_cascade_tree(&hierarchy_ref, &depths, &data_ref);

        assert_eq!(out.len(), DEPTH);
        for i in 0..DEPTH {
            assert_eq!(
                out.internal[i],
                info(0, true),
                "every node in a chain is an only child"
            );
        }
    }

    // ---------------------------------------------------------------------
    // matches_html_element
    // ---------------------------------------------------------------------

    #[test]
    fn matches_html_element_empty_path_never_matches() {
        assert!(!matches(Vec::new(), 1, None));
        assert!(!matches(Vec::new(), 0, Some(CssPathPseudoSelector::Hover)));
    }

    #[test]
    fn matches_html_element_matches_type_class_and_id_on_the_subject() {
        assert!(matches(vec![CssPathSelector::Global], 1, None));
        assert!(matches(vec![CssPathSelector::Class("a".into())], 1, None));
        assert!(!matches(vec![CssPathSelector::Class("a".into())], 3, None));
        assert!(matches(vec![CssPathSelector::Id("first".into())], 1, None));
        assert!(matches(
            vec![CssPathSelector::Type(NodeTypeTag::Div)],
            1,
            None
        ));
        assert!(!matches(
            vec![CssPathSelector::Type(NodeTypeTag::Div)],
            0,
            None
        ));
        assert!(matches(
            vec![CssPathSelector::Type(NodeTypeTag::Body)],
            0,
            None
        ));

        // Compound group: `div.a` matches node 1 but not node 3 (`div.b`).
        let div_a = vec![
            CssPathSelector::Type(NodeTypeTag::Div),
            CssPathSelector::Class("a".into()),
        ];
        assert!(matches(div_a.clone(), 1, None));
        assert!(!matches(div_a, 3, None));
    }

    #[test]
    fn matches_html_element_never_matches_an_anonymous_node() {
        let (hierarchy, data) = anonymous_fixture();
        let hier_items = items(&hierarchy);
        let hierarchy_ref = NodeHierarchyRef::from_slice(&hierarchy);
        let data_ref = NodeDataContainerRef::from_slice(&data);
        let depths = hierarchy_ref.get_parents_sorted_by_depth();
        let cascade = construct_html_cascade_tree(&hierarchy_ref, &depths, &data_ref);

        // `*` matches everything EXCEPT the anonymous boxes (1, 2, 4).
        for id in [1usize, 2, 4] {
            assert!(
                !matches_html_element(
                    &CssPath::new(vec![CssPathSelector::Global]),
                    NodeId::new(id),
                    &NodeDataContainerRef::from_slice(&hier_items),
                    &data_ref,
                    &cascade.as_ref(),
                    None,
                ),
                "anonymous node {id} must not be styled"
            );
        }
        for id in [0usize, 3, 5] {
            assert!(matches_html_element(
                &CssPath::new(vec![CssPathSelector::Global]),
                NodeId::new(id),
                &NodeDataContainerRef::from_slice(&hier_items),
                &data_ref,
                &cascade.as_ref(),
                None,
            ));
        }
    }

    #[test]
    fn matches_html_element_child_combinator_is_stricter_than_the_descendant_one() {
        // `body > p` must NOT match p.inner (its parent is div.b) ...
        let child = vec![
            CssPathSelector::Type(NodeTypeTag::Body),
            CssPathSelector::DirectChildren,
            CssPathSelector::Type(NodeTypeTag::P),
        ];
        assert!(!matches(child, 4, None));

        // ... but `body p` must.
        let descendant = vec![
            CssPathSelector::Type(NodeTypeTag::Body),
            CssPathSelector::Children,
            CssPathSelector::Type(NodeTypeTag::P),
        ];
        assert!(matches(descendant, 4, None));

        // `body > div.b` is a direct child.
        let direct = vec![
            CssPathSelector::Type(NodeTypeTag::Body),
            CssPathSelector::DirectChildren,
            CssPathSelector::Type(NodeTypeTag::Div),
            CssPathSelector::Class("b".into()),
        ];
        assert!(matches(direct, 3, None));
    }

    #[test]
    fn matches_html_element_descendant_combinator_walks_the_whole_ancestor_chain() {
        // `div.b p.inner` (direct) and `body p.inner` (two levels up).
        let close = vec![
            CssPathSelector::Class("b".into()),
            CssPathSelector::Children,
            CssPathSelector::Class("inner".into()),
        ];
        assert!(matches(close, 4, None));

        // A non-ancestor class must not match, even though it exists in the DOM.
        let unrelated = vec![
            CssPathSelector::Class("c".into()),
            CssPathSelector::Children,
            CssPathSelector::Class("inner".into()),
        ];
        assert!(!matches(unrelated, 4, None));
    }

    #[test]
    fn matches_html_element_general_sibling_scans_all_previous_siblings() {
        // `div.a ~ div.c`: div.c (5) is preceded by div.b (3) and a text node (2),
        // and the scan must keep walking until it reaches div.a (1).
        let general = vec![
            CssPathSelector::Class("a".into()),
            CssPathSelector::GeneralSibling,
            CssPathSelector::Class("c".into()),
        ];
        assert!(matches(general, 5, None));

        // The subject must come AFTER the sibling: `div.c ~ div.a` must not match.
        let backwards = vec![
            CssPathSelector::Class("c".into()),
            CssPathSelector::GeneralSibling,
            CssPathSelector::Class("a".into()),
        ];
        assert!(!matches(backwards, 1, None));
    }

    #[test]
    fn matches_html_element_adjacent_sibling_matches_the_immediate_element_sibling() {
        // `div.b + div.c`: node 5's immediately preceding sibling IS div.b.
        let adjacent = vec![
            CssPathSelector::Class("b".into()),
            CssPathSelector::AdjacentSibling,
            CssPathSelector::Class("c".into()),
        ];
        assert!(matches(adjacent, 5, None));

        // `div.a + div.c` must not match (div.b sits between them).
        let not_adjacent = vec![
            CssPathSelector::Class("a".into()),
            CssPathSelector::AdjacentSibling,
            CssPathSelector::Class("c".into()),
        ];
        assert!(!matches(not_adjacent, 5, None));
    }

    /// EXPECTED-RED (genuine bug, see report): `find_non_anonymous_prev_sibling`
    /// only skips *anonymous* nodes, not *non-element* (text) nodes. CSS
    /// Selectors L4 §15.2 defines `E + F` over element siblings only — and
    /// `construct_html_cascade_tree` already excludes text nodes from sibling
    /// indexing (L4 §13) — so `div.a + div.b` must still match across the
    /// intervening text node. Today it returns `false`.
    #[test]
    fn matches_html_element_adjacent_sibling_skips_text_nodes() {
        // `div.a + div.b`, with the text node 2 sitting between them.
        let adjacent = vec![
            CssPathSelector::Class("a".into()),
            CssPathSelector::AdjacentSibling,
            CssPathSelector::Class("b".into()),
        ];
        assert!(
            matches(adjacent, 3, None),
            "the `+` combinator must ignore non-element (text) siblings"
        );
    }

    #[test]
    fn matches_html_element_hover_on_a_single_group_path_needs_the_expected_ending() {
        let hover = vec![
            CssPathSelector::Class("a".into()),
            CssPathSelector::PseudoSelector(CssPathPseudoSelector::Hover),
        ];
        assert!(matches(
            hover.clone(),
            1,
            Some(CssPathPseudoSelector::Hover)
        ));
        // Without the expected pseudo state the rule must not apply...
        assert!(!matches(hover.clone(), 1, None));
        // ... and neither must a different state.
        assert!(!matches(
            hover.clone(),
            1,
            Some(CssPathPseudoSelector::Focus)
        ));
        // ... and it still has to match the rest of the selector.
        assert!(!matches(hover, 3, Some(CssPathPseudoSelector::Hover)));
    }

    /// EXPECTED-RED (genuine bug, see report): `matches_html_element` passes
    /// `is_last_content_group = groups.len() == 1` for the SUBJECT group (the
    /// rightmost one, which the iterator yields first), so an interactive pseudo
    /// on the subject of any multi-group selector — `.container .btn:hover`,
    /// `body > .btn:hover`, … — can never match. `prop_cache` reaches
    /// `matches_html_element` with exactly this shape (`rule_ends_with(path,
    /// Some(Hover))` → `matches_html_element(..., Some(Hover))`), so every
    /// descendant/child `:hover` / `:active` / `:focus` rule is silently dropped.
    #[test]
    fn matches_html_element_hover_on_a_descendant_path_still_matches() {
        // `body .a:hover` — the hover applies to the SUBJECT (node 1).
        let hover_descendant = vec![
            CssPathSelector::Type(NodeTypeTag::Body),
            CssPathSelector::Children,
            CssPathSelector::Class("a".into()),
            CssPathSelector::PseudoSelector(CssPathPseudoSelector::Hover),
        ];
        assert!(
            matches(hover_descendant, 1, Some(CssPathPseudoSelector::Hover)),
            ":hover on the subject of a multi-group selector must still match"
        );
    }

    #[test]
    fn matches_html_element_structural_pseudos_work_on_the_subject() {
        // div#first.a is the first element child; div.c is the last one.
        let first = vec![
            CssPathSelector::Class("a".into()),
            CssPathSelector::PseudoSelector(CssPathPseudoSelector::First),
        ];
        assert!(matches(first, 1, None));

        let last = vec![
            CssPathSelector::Class("c".into()),
            CssPathSelector::PseudoSelector(CssPathPseudoSelector::Last),
        ];
        assert!(matches(last, 5, None));

        // div.b is the 2nd element child — the text node must not shift the index.
        let nth = vec![
            CssPathSelector::Class("b".into()),
            CssPathSelector::PseudoSelector(CssPathPseudoSelector::NthChild(
                CssNthChildSelector::Number(2),
            )),
        ];
        assert!(matches(nth, 3, None));

        let wrong_nth = vec![
            CssPathSelector::Class("b".into()),
            CssPathSelector::PseudoSelector(CssPathPseudoSelector::NthChild(
                CssNthChildSelector::Number(3),
            )),
        ];
        assert!(!matches(wrong_nth, 3, None));
    }

    #[test]
    fn matches_html_element_root_scope_confines_a_rule_to_its_subtree() {
        // `[Root(3..=4), *]` — the #47 scope marker: only div.b and its child.
        let scoped = vec![
            CssPathSelector::Root(CssScopeRange { start: 3, end: 4 }),
            CssPathSelector::Global,
        ];
        assert!(matches(scoped.clone(), 3, None));
        assert!(matches(scoped.clone(), 4, None));
        assert!(
            !matches(scoped.clone(), 1, None),
            "must not leak to a sibling"
        );
        assert!(
            !matches(scoped.clone(), 5, None),
            "must not leak to a sibling"
        );
        assert!(!matches(scoped, 0, None), "must not leak to the parent");

        // Node-only scope (`[start, start]`) = inline-style semantics.
        let node_only = vec![
            CssPathSelector::Root(CssScopeRange { start: 3, end: 3 }),
            CssPathSelector::Global,
        ];
        assert!(matches(node_only.clone(), 3, None));
        assert!(
            !matches(node_only, 4, None),
            "a node-only scope must not reach children"
        );
    }

    #[test]
    fn matches_html_element_with_a_dangling_combinator_does_not_panic() {
        // `.a >` — the iterator yields an empty subject group, which matches
        // vacuously, and then requires an `.a` parent. Node 4's parent is div.b,
        // so this must be false; no panic either way.
        let dangling = vec![
            CssPathSelector::Class("a".into()),
            CssPathSelector::DirectChildren,
        ];
        assert!(!matches(dangling.clone(), 4, None));
        // ... but div.a IS the parent of nothing, so no node matches it.
        for id in 0..6 {
            let _ = matches(dangling.clone(), id, None);
        }
    }

    #[test]
    fn matches_html_element_on_a_very_long_selector_chain_terminates() {
        // 500 `body ...` descendant groups: the ancestor scan must fail fast
        // (there are only 3 levels in the DOM) instead of looping.
        let mut path = Vec::new();
        for _ in 0..500 {
            path.push(CssPathSelector::Type(NodeTypeTag::Body));
            path.push(CssPathSelector::Children);
        }
        path.push(CssPathSelector::Class("inner".into()));
        assert!(!matches(path, 4, None));
    }
}
