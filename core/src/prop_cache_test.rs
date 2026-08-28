#[cfg(test)]
#[allow(clippy::float_cmp, clippy::too_many_lines)]
mod autotest_generated {
    use azul_css::{
        css::CssPropertyValue,
        dynamic_selector::{
            CssPropertyWithConditions, DynamicSelector, DynamicSelectorContext, PseudoStateType,
        },
        props::{
            basic::{length::SizeMetric, pixel::PixelValue},
            layout::{
                LayoutFlexBasis, LayoutInsetBottom, LayoutLeft, LayoutMarginTop, LayoutMaxWidth,
                LayoutMinWidth, LayoutOverflow, LayoutPaddingLeft, LayoutRight, LayoutTop,
            },
            style::LayoutBorderLeftWidth,
        },
    };

    use super::*;

    // ---------------------------------------------------------------------
    // helpers
    // ---------------------------------------------------------------------

    /// Approximate float compare — every value here round-trips through
    /// `FloatValue`'s fixed-point (1/1000) encoding.
    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    fn n0() -> NodeId {
        NodeId::new(0)
    }

    fn normal() -> StyledNodeState {
        StyledNodeState::default()
    }

    /// A `<div>` carrying `props` as unconditional (Normal-state) inline CSS.
    fn div_with(props: Vec<CssProperty>) -> NodeData {
        let mut nd = NodeData::create_div();
        for property in props {
            nd.add_css_property(CssPropertyWithConditions {
                property,
                apply_if: Vec::new().into(),
            });
        }
        nd
    }

    /// A `<div>` carrying `props` gated on a single pseudo-state.
    fn div_with_pseudo(props: Vec<CssProperty>, state: PseudoStateType) -> NodeData {
        let mut nd = NodeData::create_div();
        for property in props {
            nd.add_css_property(CssPropertyWithConditions {
                property,
                apply_if: vec![DynamicSelector::PseudoState(state)].into(),
            });
        }
        nd
    }

    fn width_px(v: f32) -> CssProperty {
        CssProperty::Width(CssPropertyValue::Exact(LayoutWidth::Px(PixelValue::px(v))))
    }

    fn width_pct(v: f32) -> CssProperty {
        CssProperty::Width(CssPropertyValue::Exact(LayoutWidth::Px(
            PixelValue::percent(v),
        )))
    }

    fn font_size(pv: PixelValue) -> CssProperty {
        CssProperty::FontSize(CssPropertyValue::Exact(StyleFontSize { inner: pv }))
    }

    /// Pull `(metric, number)` back out of a `CssProperty::FontSize`.
    fn font_size_parts(p: &CssProperty) -> Option<(SizeMetric, f32)> {
        match p {
            CssProperty::FontSize(v) => v
                .get_property()
                .map(|fs| (fs.inner.metric, fs.inner.number.get())),
            _ => None,
        }
    }

    fn stateful(state: PseudoStateType, property: CssProperty) -> StatefulCssProperty {
        StatefulCssProperty {
            state,
            prop_type: property.get_type(),
            property,
        }
    }

    // =====================================================================
    // FlatVecVec — construction / getters / predicates
    // =====================================================================

    #[test]
    fn flatvecvec_new_zero_is_empty() {
        let f = FlatVecVec::<i32>::new(0);
        assert_eq!(f.len(), 0);
        assert!(f.is_empty());
        // Quirk worth pinning: with no build slots at all, `is_flattened()` is
        // vacuously true (`build.is_empty()`), even though `flatten()` never ran.
        assert!(f.is_flattened());
        assert!(f.get_slice(0).is_empty());
    }

    #[test]
    fn flatvecvec_new_invariants_hold() {
        let f = FlatVecVec::<i32>::new(3);
        assert_eq!(f.len(), 3);
        assert!(!f.is_empty());
        assert!(!f.is_flattened(), "fresh multi-slot vec is in build phase");
        assert_eq!(f.build_get(0), Some(&Vec::new()));
        assert_eq!(f.build_get(2), Some(&Vec::new()));
        assert_eq!(f.build_get(3), None, "one past the end");
        assert_eq!(f.build_get(usize::MAX), None);
        assert!(f.get_slice(0).is_empty());
    }

    #[test]
    fn flatvecvec_default_is_neutral() {
        let f = FlatVecVec::<i32>::default();
        assert_eq!(f.len(), 0);
        assert!(f.is_empty());
        assert_eq!(f.build_get(0), None);
        assert!(f.get_slice(0).is_empty());
    }

    #[test]
    fn flatvecvec_get_slice_out_of_bounds_is_empty_in_both_phases() {
        let mut f = FlatVecVec::<i32>::new(2);
        f.push_to(0, 7);
        // build phase
        assert_eq!(f.get_slice(0), &[7]);
        assert!(f.get_slice(2).is_empty());
        assert!(f.get_slice(usize::MAX).is_empty());

        f.flatten();
        // read phase — same out-of-bounds contract, still no panic
        assert_eq!(f.get_slice(0), &[7]);
        assert!(f.get_slice(2).is_empty());
        assert!(f.get_slice(usize::MAX).is_empty());
    }

    #[test]
    #[should_panic(expected = "index out of bounds")]
    fn flatvecvec_push_to_out_of_bounds_panics() {
        // Documented in `push_to`: "Panics if ... node_index >= len()".
        let mut f = FlatVecVec::<i32>::new(1);
        f.push_to(1, 0);
    }

    #[test]
    #[should_panic(expected = "index out of bounds")]
    fn flatvecvec_push_to_after_flatten_panics() {
        // Documented in `push_to`: "Panics if already flattened".
        let mut f = FlatVecVec::<i32>::new(1);
        f.flatten();
        f.push_to(0, 0);
    }

    #[test]
    #[should_panic(expected = "index out of bounds")]
    fn flatvecvec_build_mut_out_of_bounds_panics() {
        let mut f = FlatVecVec::<i32>::new(1);
        let _ = f.build_mut(usize::MAX);
    }

    #[test]
    fn flatvecvec_build_iter_mut_visits_every_slot() {
        let mut f = FlatVecVec::<i32>::new(3);
        f.push_to(0, 1);
        f.push_to(2, 2);
        let mut visited = 0;
        for v in f.build_iter_mut() {
            visited += 1;
            v.clear();
        }
        assert_eq!(visited, 3);
        assert!(f.get_slice(0).is_empty());
        assert!(f.get_slice(2).is_empty());
    }

    #[test]
    fn flatvecvec_build_get_returns_none_once_flattened() {
        let mut f = FlatVecVec::<i32>::new(1);
        f.push_to(0, 5);
        f.flatten();
        // Doc: "During read phase, returns None (use `get_slice` instead)."
        assert_eq!(f.build_get(0), None);
        assert_eq!(f.get_slice(0), &[5]);
    }

    // =====================================================================
    // FlatVecVec — heap_bytes (numeric)
    // =====================================================================

    #[test]
    fn flatvecvec_heap_bytes_zero_and_empty() {
        let f = FlatVecVec::<i32>::default();
        assert_eq!(f.heap_bytes(0), 0, "empty vec, zero element size");
        // All three capacities are 0, so even a nonsensical MAX element size
        // multiplies out to 0 rather than overflowing.
        assert_eq!(f.heap_bytes(usize::MAX), 0);
        assert_eq!(f.heap_bytes(size_of::<i32>()), 0);
    }

    #[test]
    fn flatvecvec_heap_bytes_counts_build_and_flat_storage() {
        let mut f = FlatVecVec::<i32>::new(4);
        // Build-phase slots cost at least the outer Vec headers, even at a
        // per-element size of 0.
        assert!(f.heap_bytes(0) >= 4 * size_of::<Vec<i32>>());

        f.push_to(0, 1);
        f.push_to(0, 2);
        let build_bytes = f.heap_bytes(size_of::<i32>());
        assert!(build_bytes > 0);

        f.flatten();
        // Flat storage accounts for the 2 elements + the 4-entry offset table.
        let flat_bytes = f.heap_bytes(size_of::<i32>());
        assert!(flat_bytes >= 2 * size_of::<i32>() + 4 * size_of::<(u32, u32)>());
    }

    // =====================================================================
    // FlatVecVec — flatten / sort_each_and_flatten
    // =====================================================================

    #[test]
    fn flatvecvec_sort_each_and_flatten_keeps_last_of_equal_keys() {
        // CSS cascade rule: among equal keys, later source order wins.
        let mut f = FlatVecVec::<(i32, i32)>::new(1);
        f.push_to(0, (1, 10));
        f.push_to(0, (1, 20)); // same key, pushed later => must win
        f.push_to(0, (0, 30));
        f.sort_each_and_flatten(|p| p.0);

        assert!(f.is_flattened());
        assert_eq!(f.get_slice(0), &[(0, 30), (1, 20)]);
    }

    #[test]
    fn flatvecvec_sort_each_and_flatten_on_empty_slots() {
        let mut f = FlatVecVec::<i32>::new(3);
        f.push_to(1, 42);
        f.sort_each_and_flatten(|v| *v);
        assert_eq!(f.len(), 3);
        assert!(f.get_slice(0).is_empty());
        assert_eq!(f.get_slice(1), &[42]);
        assert!(f.get_slice(2).is_empty());
    }

    #[test]
    fn flatvecvec_sort_each_and_flatten_on_zero_nodes_does_not_panic() {
        let mut f = FlatVecVec::<i32>::new(0);
        f.sort_each_and_flatten(|v| *v);
        assert_eq!(f.len(), 0);
        assert!(f.get_slice(0).is_empty());
    }

    #[test]
    fn flatvecvec_flatten_does_not_deduplicate() {
        let mut f = FlatVecVec::<i32>::new(2);
        f.push_to(0, 5);
        f.push_to(0, 5);
        f.push_to(1, 9);
        f.flatten();
        assert!(f.is_flattened());
        assert_eq!(f.get_slice(0), &[5, 5], "flatten() must not dedup");
        assert_eq!(f.get_slice(1), &[9]);
    }

    // =====================================================================
    // FlatVecVec — retain
    // =====================================================================

    #[test]
    fn flatvecvec_retain_before_flatten_is_a_noop() {
        // Doc: "Must be called after flatten." Before that it must not silently
        // corrupt the build-phase data — it early-returns.
        let mut f = FlatVecVec::<i32>::new(1);
        f.push_to(0, 1);
        f.push_to(0, 2);
        f.retain(|_| false);
        assert_eq!(f.get_slice(0), &[1, 2], "build-phase data left untouched");
    }

    #[test]
    fn flatvecvec_retain_preserves_per_node_order() {
        let mut f = FlatVecVec::<i32>::new(2);
        for v in [1, 2, 3, 4] {
            f.push_to(0, v);
        }
        f.push_to(1, 5);
        f.flatten();

        f.retain(|v| v % 2 == 0);
        assert_eq!(f.get_slice(0), &[2, 4]);
        assert!(f.get_slice(1).is_empty());
        assert_eq!(f.len(), 2, "node slots survive an empty retain");
    }

    #[test]
    fn flatvecvec_retain_dropping_everything_leaves_empty_slices() {
        let mut f = FlatVecVec::<i32>::new(2);
        f.push_to(0, 1);
        f.push_to(1, 2);
        f.flatten();
        f.retain(|_| false);
        assert_eq!(f.len(), 2);
        assert!(f.get_slice(0).is_empty());
        assert!(f.get_slice(1).is_empty());
    }

    #[test]
    fn flatvecvec_retain_with_node_index_sees_owning_node() {
        let mut f = FlatVecVec::<i32>::new(3);
        f.push_to(0, 10);
        f.push_to(1, 11);
        f.push_to(2, 12);
        f.flatten();

        f.retain_with_node_index(|idx, _| idx == 1);
        assert!(f.get_slice(0).is_empty());
        assert_eq!(f.get_slice(1), &[11]);
        assert!(f.get_slice(2).is_empty());
    }

    #[test]
    fn flatvecvec_retain_with_node_index_before_flatten_is_a_noop() {
        let mut f = FlatVecVec::<i32>::new(1);
        f.push_to(0, 1);
        f.retain_with_node_index(|_, _| false);
        assert_eq!(f.get_slice(0), &[1]);
    }

    // =====================================================================
    // FlatVecVec — iteration / extend_from
    // =====================================================================

    #[test]
    fn flatvecvec_iter_node_slices_covers_all_nodes_in_both_phases() {
        let mut f = FlatVecVec::<i32>::new(3);
        f.push_to(1, 7);

        let build: Vec<(usize, Vec<i32>)> =
            f.iter_node_slices().map(|(i, s)| (i, s.to_vec())).collect();
        assert_eq!(build, vec![(0, vec![]), (1, vec![7]), (2, vec![])]);

        f.flatten();
        let flat: Vec<(usize, Vec<i32>)> =
            f.iter_node_slices().map(|(i, s)| (i, s.to_vec())).collect();
        assert_eq!(flat, build, "iteration is phase-independent");
    }

    #[test]
    fn flatvecvec_iter_node_slices_on_empty_yields_nothing() {
        let f = FlatVecVec::<i32>::new(0);
        assert_eq!(f.iter_node_slices().count(), 0);
    }

    #[test]
    fn flatvecvec_extend_from_both_in_build_phase() {
        let mut a = FlatVecVec::<i32>::new(1);
        a.push_to(0, 1);
        let mut b = FlatVecVec::<i32>::new(2);
        b.push_to(0, 2);
        b.push_to(1, 3);

        a.extend_from(&mut b);
        assert_eq!(a.len(), 3);
        assert_eq!(a.get_slice(0), &[1]);
        assert_eq!(a.get_slice(1), &[2]);
        assert_eq!(a.get_slice(2), &[3]);
        assert_eq!(b.len(), 0, "other is drained");
    }

    #[test]
    fn flatvecvec_extend_from_both_flattened_rebases_offsets() {
        let mut a = FlatVecVec::<i32>::new(2);
        a.push_to(0, 1);
        a.push_to(1, 2);
        a.flatten();

        let mut b = FlatVecVec::<i32>::new(2);
        b.push_to(0, 3);
        b.push_to(1, 4);
        b.flatten();

        a.extend_from(&mut b);
        assert_eq!(a.len(), 4);
        assert_eq!(a.get_slice(0), &[1]);
        assert_eq!(a.get_slice(1), &[2]);
        assert_eq!(a.get_slice(2), &[3], "offsets rebased onto a's flat data");
        assert_eq!(a.get_slice(3), &[4]);
    }

    #[test]
    fn flatvecvec_extend_from_across_phases_discards_self_flat_data() {
        // Doc precondition: "Both must be in build phase, or both must be
        // flattened." This pins what a violation actually does today — the
        // flattened side's items are dropped on the floor rather than merged.
        let mut a = FlatVecVec::<i32>::new(1);
        a.push_to(0, 1);
        a.flatten();

        let mut b = FlatVecVec::<i32>::new(1);
        b.push_to(0, 2);

        a.extend_from(&mut b); // no panic...
        assert_eq!(a.len(), 1);
        assert_eq!(
            a.get_slice(0),
            &[2],
            "a's own flattened item (1) is silently lost"
        );
    }

    #[test]
    fn flatvecvec_eq_within_the_same_phase() {
        let mut a = FlatVecVec::<i32>::new(1);
        a.push_to(0, 1);
        let mut b = FlatVecVec::<i32>::new(1);
        b.push_to(0, 1);
        assert_eq!(a, b);

        b.push_to(0, 2);
        assert_ne!(a, b);

        a.flatten();
        // (a and c are both flattened below — equality is only meaningful
        // between two caches in the same phase)
        let mut c = FlatVecVec::<i32>::new(1);
        c.push_to(0, 1);
        c.flatten();
        assert_eq!(a, c);
    }

    // =====================================================================
    // CssPropertyCacheBreakdown
    // =====================================================================

    #[test]
    fn breakdown_total_bytes_sums_subfields_and_excludes_node_count() {
        let b = CssPropertyCacheBreakdown {
            node_count: 999_999,
            cascaded_props_bytes: 1,
            css_props_bytes: 2,
            computed_values_bytes: 4,
            user_overridden_bytes: 8,
            global_css_props_bytes: 16,
            compact_cache_bytes: 32,
            resolved_font_sizes_bytes: 64,
        };
        assert_eq!(b.total_bytes(), 127, "node_count is not a byte count");
    }

    #[test]
    fn breakdown_total_bytes_default_is_zero_and_max_single_field_does_not_overflow() {
        assert_eq!(CssPropertyCacheBreakdown::default().total_bytes(), 0);

        let b = CssPropertyCacheBreakdown {
            cascaded_props_bytes: usize::MAX,
            ..Default::default()
        };
        assert_eq!(b.total_bytes(), usize::MAX);
    }

    // =====================================================================
    // CssPropertyCache — construction / memory / append
    // =====================================================================

    #[test]
    fn cache_empty_zero_is_neutral() {
        let c = CssPropertyCache::empty(0);
        assert_eq!(c.node_count, 0);
        assert!(c.css_props.is_empty());
        assert!(c.cascaded_props.is_empty());
        assert!(c.computed_values.is_empty());
        assert!(c.user_overridden_properties.is_empty());
        assert!(c.global_css_props.is_empty());
        assert!(c.compact_cache.is_none());

        let b = c.memory_breakdown();
        assert_eq!(b.node_count, 0);
        assert_eq!(b.total_bytes(), 0, "a zero-node cache retains no heap");
    }

    #[test]
    fn cache_empty_invariants_hold() {
        let c = CssPropertyCache::empty(7);
        assert_eq!(c.node_count, 7);
        assert_eq!(c.css_props.len(), 7);
        assert_eq!(c.cascaded_props.len(), 7);
        assert!(!c.css_props.is_flattened(), "starts in build phase");
        assert!(c.compact_cache.is_none());

        let b = c.memory_breakdown();
        assert_eq!(b.node_count, 7);
        assert!(b.total_bytes() > 0);
        assert_eq!(b.compact_cache_bytes, 0);
        assert_eq!(b.resolved_font_sizes_bytes, 0);
    }

    #[test]
    fn cache_invalidate_resolved_font_sizes_clears_the_once_lock() {
        let mut c = CssPropertyCache::empty(1);
        assert!(c.resolved_font_sizes_px.set(vec![16.0]).is_ok());
        assert!(c.resolved_font_sizes_px.get().is_some());

        c.invalidate_resolved_font_sizes();
        assert!(
            c.resolved_font_sizes_px.get().is_none(),
            "next read must recompute"
        );
        // and it can be re-populated afterwards
        assert!(c.resolved_font_sizes_px.set(vec![12.0]).is_ok());
    }

    #[test]
    fn cache_append_sums_nodes_and_invalidates_derived_caches() {
        let mut a = CssPropertyCache::empty(2);
        let mut b = CssPropertyCache::empty(3);
        assert!(a.resolved_font_sizes_px.set(vec![16.0, 16.0]).is_ok());

        a.append(&mut b);

        assert_eq!(a.node_count, 5);
        assert_eq!(a.css_props.len(), 5);
        assert_eq!(a.cascaded_props.len(), 5);
        assert!(
            a.resolved_font_sizes_px.get().is_none(),
            "node indices shifted"
        );
        assert!(a.compact_cache.is_none());
    }

    #[test]
    fn cache_append_of_empty_cache_is_a_noop_on_node_count() {
        let mut a = CssPropertyCache::empty(2);
        let mut b = CssPropertyCache::empty(0);
        a.append(&mut b);
        assert_eq!(a.node_count, 2);
        assert_eq!(a.css_props.len(), 2);
    }

    #[test]
    fn cache_invalidate_resolved_cache_drops_compact_cache() {
        let mut c = CssPropertyCache::empty(1);
        c.invalidate_resolved_cache();
        assert!(c.compact_cache.is_none());
    }

    #[test]
    fn cache_ptr_new_and_downcast_roundtrip() {
        let mut p = CssPropertyCachePtr::new(CssPropertyCache::empty(4));
        assert!(p.run_destructor);
        assert_eq!(p.downcast_mut().node_count, 4);

        p.downcast_mut().node_count = 9;
        assert_eq!(
            p.downcast_mut().node_count,
            9,
            "downcast_mut aliases the box"
        );
    }

    // =====================================================================
    // Predicates (overflow / border / box-shadow)
    // =====================================================================

    #[test]
    fn overflow_predicates_default_to_visible_for_a_bare_div() {
        let c = CssPropertyCache::empty(1);
        let nd = NodeData::create_div();
        assert!(c.is_horizontal_overflow_visible(&nd, &n0(), &normal()));
        assert!(c.is_vertical_overflow_visible(&nd, &n0(), &normal()));
        assert!(!c.is_horizontal_overflow_hidden(&nd, &n0(), &normal()));
        assert!(!c.is_vertical_overflow_hidden(&nd, &n0(), &normal()));
    }

    #[test]
    fn overflow_predicates_are_per_axis() {
        let c = CssPropertyCache::empty(1);
        let nd = div_with(vec![CssProperty::OverflowX(CssPropertyValue::Exact(
            LayoutOverflow::Hidden,
        ))]);
        assert!(c.is_horizontal_overflow_hidden(&nd, &n0(), &normal()));
        assert!(!c.is_horizontal_overflow_visible(&nd, &n0(), &normal()));
        // the Y axis must be untouched
        assert!(!c.is_vertical_overflow_hidden(&nd, &n0(), &normal()));
        assert!(c.is_vertical_overflow_visible(&nd, &n0(), &normal()));
    }

    #[test]
    fn overflow_predicates_do_not_panic_on_an_out_of_range_node_id() {
        let c = CssPropertyCache::empty(0);
        let nd = NodeData::create_div();
        let far = NodeId::new(999_999);
        assert!(c.is_horizontal_overflow_visible(&nd, &far, &normal()));
        assert!(!c.is_vertical_overflow_hidden(&nd, &far, &normal()));
    }

    #[test]
    fn has_border_false_without_and_true_with_a_border_width() {
        let c = CssPropertyCache::empty(1);
        assert!(!c.has_border(&NodeData::create_div(), &n0(), &normal()));

        let bordered = div_with(vec![CssProperty::BorderLeftWidth(CssPropertyValue::Exact(
            LayoutBorderLeftWidth {
                inner: PixelValue::px(2.0),
            },
        ))]);
        assert!(c.has_border(&bordered, &n0(), &normal()));
    }

    #[test]
    fn has_box_shadow_false_for_a_bare_div() {
        let c = CssPropertyCache::empty(1);
        assert!(!c.has_box_shadow(&NodeData::create_div(), &n0(), &normal()));
        // out-of-range node id must not panic either
        assert!(!c.has_box_shadow(&NodeData::create_div(), &NodeId::new(500), &normal()));
    }

    // =====================================================================
    // `*_or_default` getters
    // =====================================================================

    #[test]
    fn or_default_getters_fall_back_to_the_css_defaults() {
        let c = CssPropertyCache::empty(1);
        let nd = NodeData::create_div();

        assert_eq!(
            c.get_font_size_or_default(&nd, &n0(), &normal()),
            azul_css::defaults::DEFAULT_FONT_SIZE
        );
        assert_eq!(
            c.get_text_color_or_default(&nd, &n0(), &normal()),
            azul_css::defaults::DEFAULT_TEXT_COLOR
        );

        let fams = c.get_font_id_or_default(&nd, &n0(), &normal());
        assert_eq!(fams.as_ref().len(), 1);
        match &fams.as_ref()[0] {
            StyleFontFamily::System(s) => {
                assert_eq!(s.as_str(), azul_css::defaults::DEFAULT_FONT_ID);
            }
            other => panic!("expected the default System font family, got {other:?}"),
        }
    }

    #[test]
    fn get_font_size_or_default_prefers_the_inline_value() {
        let c = CssPropertyCache::empty(1);
        let nd = div_with(vec![font_size(PixelValue::px(42.0))]);
        let fs = c.get_font_size_or_default(&nd, &n0(), &normal());
        assert!(close(fs.inner.number.get(), 42.0));
        assert_eq!(fs.inner.metric, SizeMetric::Px);
    }

    #[test]
    fn or_default_getters_survive_an_out_of_range_node_id() {
        let c = CssPropertyCache::empty(0);
        let nd = NodeData::create_div();
        let far = NodeId::new(usize::MAX / 2);
        assert_eq!(
            c.get_font_size_or_default(&nd, &far, &normal()),
            azul_css::defaults::DEFAULT_FONT_SIZE
        );
        assert_eq!(
            c.get_font_id_or_default(&nd, &far, &normal())
                .as_ref()
                .len(),
            1
        );
    }

    // =====================================================================
    // calc_* (numeric: zero / negative / NaN / inf / saturation)
    // =====================================================================

    #[test]
    fn calc_width_is_zero_when_unset() {
        let c = CssPropertyCache::empty(1);
        let nd = NodeData::create_div();
        assert_eq!(c.calc_width(&nd, &n0(), &normal(), 800.0), 0.0);
        assert_eq!(c.calc_width(&nd, &n0(), &normal(), 0.0), 0.0);
        assert_eq!(c.calc_height(&nd, &n0(), &normal(), f32::NAN), 0.0);
    }

    #[test]
    fn calc_width_resolves_px_and_percent() {
        let c = CssPropertyCache::empty(1);

        let px = div_with(vec![width_px(100.0)]);
        assert!(close(c.calc_width(&px, &n0(), &normal(), 800.0), 100.0));
        // px must ignore the reference entirely
        assert!(close(c.calc_width(&px, &n0(), &normal(), 0.0), 100.0));

        let pct = div_with(vec![width_pct(50.0)]);
        assert!(close(c.calc_width(&pct, &n0(), &normal(), 800.0), 400.0));
        assert!(close(c.calc_width(&pct, &n0(), &normal(), 0.0), 0.0));
    }

    #[test]
    fn calc_width_with_a_negative_reference_is_negative_not_clamped() {
        let c = CssPropertyCache::empty(1);
        let pct = div_with(vec![width_pct(50.0)]);
        assert!(close(c.calc_width(&pct, &n0(), &normal(), -800.0), -400.0));
    }

    #[test]
    fn calc_width_with_nan_and_infinite_references_is_defined() {
        let c = CssPropertyCache::empty(1);
        let pct = div_with(vec![width_pct(50.0)]);

        assert!(c.calc_width(&pct, &n0(), &normal(), f32::NAN).is_nan());
        assert_eq!(
            c.calc_width(&pct, &n0(), &normal(), f32::INFINITY),
            f32::INFINITY
        );
        assert_eq!(
            c.calc_width(&pct, &n0(), &normal(), f32::NEG_INFINITY),
            f32::NEG_INFINITY
        );
    }

    #[test]
    fn calc_width_saturates_non_finite_pixel_values_at_construction() {
        let c = CssPropertyCache::empty(1);

        // PixelValue stores a fixed-point isize, so `as isize` saturates:
        // NaN => 0, +inf => isize::MAX, -inf => isize::MIN. Nothing panics and
        // nothing leaks a NaN into layout.
        let nan = div_with(vec![width_px(f32::NAN)]);
        assert_eq!(c.calc_width(&nan, &n0(), &normal(), 800.0), 0.0);

        let inf = div_with(vec![width_px(f32::INFINITY)]);
        let got = c.calc_width(&inf, &n0(), &normal(), 800.0);
        assert!(got.is_finite() && got > 0.0, "saturated, got {got}");

        let neg_inf = div_with(vec![width_px(f32::NEG_INFINITY)]);
        let got = c.calc_width(&neg_inf, &n0(), &normal(), 800.0);
        assert!(got.is_finite() && got < 0.0, "saturated, got {got}");

        let huge = div_with(vec![width_px(f32::MAX)]);
        assert!(c.calc_width(&huge, &n0(), &normal(), 800.0).is_finite());
    }

    #[test]
    fn calc_width_of_auto_and_intrinsic_keywords_is_zero() {
        let c = CssPropertyCache::empty(1);

        let auto = div_with(vec![CssProperty::Width(CssPropertyValue::Auto)]);
        assert_eq!(c.calc_width(&auto, &n0(), &normal(), 800.0), 0.0);

        // min-content/max-content are not resolvable here; documented as 0.0.
        let min_content = div_with(vec![CssProperty::Width(CssPropertyValue::Exact(
            LayoutWidth::MinContent,
        ))]);
        assert_eq!(c.calc_width(&min_content, &n0(), &normal(), 800.0), 0.0);
    }

    #[test]
    fn calc_height_mirrors_calc_width() {
        let c = CssPropertyCache::empty(1);
        let nd = div_with(vec![CssProperty::Height(CssPropertyValue::Exact(
            LayoutHeight::Px(PixelValue::percent(25.0)),
        ))]);
        assert!(close(c.calc_height(&nd, &n0(), &normal(), 400.0), 100.0));
        assert!(c.calc_height(&nd, &n0(), &normal(), f32::NAN).is_nan());
    }

    #[test]
    fn calc_min_width_defaults_to_zero_and_max_width_defaults_to_none() {
        let c = CssPropertyCache::empty(1);
        let nd = NodeData::create_div();

        assert_eq!(c.calc_min_width(&nd, &n0(), &normal(), 800.0), 0.0);
        assert_eq!(c.calc_min_height(&nd, &n0(), &normal(), 600.0), 0.0);
        assert_eq!(c.calc_max_width(&nd, &n0(), &normal(), 800.0), None);
        assert_eq!(c.calc_max_height(&nd, &n0(), &normal(), 600.0), None);
    }

    #[test]
    fn calc_min_max_width_resolve_percentages_and_propagate_nan() {
        let c = CssPropertyCache::empty(1);
        let nd = div_with(vec![
            CssProperty::MinWidth(CssPropertyValue::Exact(LayoutMinWidth {
                inner: PixelValue::percent(10.0),
            })),
            CssProperty::MaxWidth(CssPropertyValue::Exact(LayoutMaxWidth {
                inner: PixelValue::percent(90.0),
            })),
        ]);

        assert!(close(
            c.calc_min_width(&nd, &n0(), &normal(), 1000.0),
            100.0
        ));
        assert!(close(
            c.calc_max_width(&nd, &n0(), &normal(), 1000.0).unwrap(),
            900.0
        ));
        assert!(c.calc_min_width(&nd, &n0(), &normal(), f32::NAN).is_nan());
        assert!(c
            .calc_max_width(&nd, &n0(), &normal(), f32::NAN)
            .unwrap()
            .is_nan());
    }

    #[test]
    fn calc_inset_getters_are_none_when_unset_and_some_when_set() {
        let c = CssPropertyCache::empty(1);
        let bare = NodeData::create_div();
        assert_eq!(c.calc_left(&bare, &n0(), &normal(), 800.0), None);
        assert_eq!(c.calc_right(&bare, &n0(), &normal(), 800.0), None);
        assert_eq!(c.calc_top(&bare, &n0(), &normal(), 600.0), None);
        assert_eq!(c.calc_bottom(&bare, &n0(), &normal(), 600.0), None);

        let inset = div_with(vec![
            CssProperty::Left(CssPropertyValue::Exact(LayoutLeft {
                inner: PixelValue::px(5.0),
            })),
            CssProperty::Right(CssPropertyValue::Exact(LayoutRight {
                inner: PixelValue::percent(10.0),
            })),
            CssProperty::Top(CssPropertyValue::Exact(LayoutTop {
                inner: PixelValue::px(-7.0),
            })),
            CssProperty::Bottom(CssPropertyValue::Exact(LayoutInsetBottom {
                inner: PixelValue::px(0.0),
            })),
        ]);
        assert!(close(
            c.calc_left(&inset, &n0(), &normal(), 800.0).unwrap(),
            5.0
        ));
        assert!(close(
            c.calc_right(&inset, &n0(), &normal(), 800.0).unwrap(),
            80.0
        ));
        assert!(close(
            c.calc_top(&inset, &n0(), &normal(), 600.0).unwrap(),
            -7.0
        ));
        assert_eq!(c.calc_bottom(&inset, &n0(), &normal(), 600.0), Some(0.0));
    }

    #[test]
    fn calc_padding_margin_border_default_to_zero() {
        let c = CssPropertyCache::empty(1);
        let nd = NodeData::create_div();
        assert_eq!(c.calc_padding_left(&nd, &n0(), &normal(), 800.0), 0.0);
        assert_eq!(c.calc_padding_right(&nd, &n0(), &normal(), 800.0), 0.0);
        assert_eq!(c.calc_padding_top(&nd, &n0(), &normal(), 600.0), 0.0);
        assert_eq!(c.calc_padding_bottom(&nd, &n0(), &normal(), 600.0), 0.0);
        assert_eq!(c.calc_margin_left(&nd, &n0(), &normal(), 800.0), 0.0);
        assert_eq!(c.calc_margin_right(&nd, &n0(), &normal(), 800.0), 0.0);
        assert_eq!(c.calc_margin_top(&nd, &n0(), &normal(), 600.0), 0.0);
        assert_eq!(c.calc_margin_bottom(&nd, &n0(), &normal(), 600.0), 0.0);
        assert_eq!(c.calc_border_left_width(&nd, &n0(), &normal(), 800.0), 0.0);
        assert_eq!(c.calc_border_right_width(&nd, &n0(), &normal(), 800.0), 0.0);
        assert_eq!(c.calc_border_top_width(&nd, &n0(), &normal(), 600.0), 0.0);
        assert_eq!(
            c.calc_border_bottom_width(&nd, &n0(), &normal(), 600.0),
            0.0
        );
    }

    #[test]
    fn calc_padding_em_uses_the_default_font_size_not_the_reference() {
        // `calc_*` passes DEFAULT_FONT_SIZE (16px) as both em and rem resolvers,
        // so an em padding must be invariant under the reference width.
        let c = CssPropertyCache::empty(1);
        let nd = div_with(vec![CssProperty::PaddingLeft(CssPropertyValue::Exact(
            LayoutPaddingLeft {
                inner: PixelValue::em(2.0),
            },
        ))]);
        assert!(close(
            c.calc_padding_left(&nd, &n0(), &normal(), 800.0),
            32.0
        ));
        assert!(close(c.calc_padding_left(&nd, &n0(), &normal(), 0.0), 32.0));
        assert!(close(
            c.calc_padding_left(&nd, &n0(), &normal(), f32::NAN),
            32.0
        ));
    }

    #[test]
    fn calc_margin_and_border_resolve_px_and_percent() {
        let c = CssPropertyCache::empty(1);
        let nd = div_with(vec![
            CssProperty::MarginTop(CssPropertyValue::Exact(LayoutMarginTop {
                inner: PixelValue::percent(50.0),
            })),
            CssProperty::BorderLeftWidth(CssPropertyValue::Exact(LayoutBorderLeftWidth {
                inner: PixelValue::px(3.0),
            })),
        ]);
        assert!(close(
            c.calc_margin_top(&nd, &n0(), &normal(), 200.0),
            100.0
        ));
        assert!(close(
            c.calc_border_left_width(&nd, &n0(), &normal(), 800.0),
            3.0
        ));
        assert!(c.calc_margin_top(&nd, &n0(), &normal(), f32::NAN).is_nan());
    }

    #[test]
    fn calc_getters_do_not_panic_on_an_out_of_range_node_id() {
        let c = CssPropertyCache::empty(0);
        let nd = NodeData::create_div();
        let far = NodeId::new(usize::MAX / 2);
        assert_eq!(c.calc_width(&nd, &far, &normal(), 800.0), 0.0);
        assert_eq!(c.calc_max_height(&nd, &far, &normal(), 600.0), None);
        assert_eq!(c.calc_padding_top(&nd, &far, &normal(), f32::INFINITY), 0.0);
    }

    // =====================================================================
    // property_needs_slow_path_after_compact
    // =====================================================================

    #[test]
    fn slow_path_only_needed_for_non_px_pixel_values() {
        // px round-trips through the compact cache => no slow path
        assert!(!property_needs_slow_path_after_compact(&width_px(10.0)));
        // % encodes to SENTINEL => must survive the prune
        assert!(property_needs_slow_path_after_compact(&width_pct(50.0)));

        assert!(!property_needs_slow_path_after_compact(
            &CssProperty::Height(CssPropertyValue::Exact(LayoutHeight::Px(PixelValue::px(
                1.0
            ))))
        ));
        assert!(property_needs_slow_path_after_compact(
            &CssProperty::Height(CssPropertyValue::Exact(LayoutHeight::Px(PixelValue::em(
                1.0
            ))))
        ));
    }

    #[test]
    fn slow_path_covers_the_plain_pixelvalue_wrappers() {
        assert!(property_needs_slow_path_after_compact(&font_size(
            PixelValue::rem(2.0)
        )));
        assert!(!property_needs_slow_path_after_compact(&font_size(
            PixelValue::px(16.0)
        )));

        assert!(property_needs_slow_path_after_compact(
            &CssProperty::MinWidth(CssPropertyValue::Exact(LayoutMinWidth {
                inner: PixelValue::percent(10.0),
            }))
        ));
        assert!(!property_needs_slow_path_after_compact(
            &CssProperty::PaddingLeft(CssPropertyValue::Exact(LayoutPaddingLeft {
                inner: PixelValue::px(4.0),
            }))
        ));
    }

    #[test]
    fn slow_path_handles_flex_basis_and_non_pixel_properties() {
        assert!(property_needs_slow_path_after_compact(
            &CssProperty::FlexBasis(CssPropertyValue::Exact(LayoutFlexBasis::Exact(
                PixelValue::percent(50.0)
            )))
        ));
        assert!(!property_needs_slow_path_after_compact(
            &CssProperty::FlexBasis(CssPropertyValue::Exact(LayoutFlexBasis::Auto))
        ));

        // Non-Exact keywords and non-pixel properties never need the slow path.
        assert!(!property_needs_slow_path_after_compact(
            &CssProperty::Width(CssPropertyValue::Auto)
        ));
        assert!(!property_needs_slow_path_after_compact(
            &CssProperty::const_none(CssPropertyType::Display)
        ));
        assert!(!property_needs_slow_path_after_compact(
            &CssProperty::const_none(CssPropertyType::BackgroundContent)
        ));
    }

    // =====================================================================
    // clone_inheritable_property (round-trip)
    // =====================================================================

    #[test]
    fn clone_inheritable_property_round_trips_heap_and_pod_variants() {
        // The whole point of this hand-rolled clone is that it must be
        // byte-equivalent to the derived Clone on native.
        let font_family = CssProperty::FontFamily(CssPropertyValue::Exact(
            vec![StyleFontFamily::System(AzString::from_const_str("serif"))].into(),
        ));
        assert_eq!(clone_inheritable_property(&font_family), font_family);

        for p in [
            CssProperty::const_none(CssPropertyType::Cursor),
            CssProperty::const_none(CssPropertyType::TextColor),
            CssProperty::const_none(CssPropertyType::BackgroundContent),
            CssProperty::const_none(CssPropertyType::Transform),
            CssProperty::const_none(CssPropertyType::Content),
            width_px(3.0),
            font_size(PixelValue::em(1.5)),
        ] {
            assert_eq!(clone_inheritable_property(&p), p, "clone must be identity");
            assert_eq!(clone_inheritable_property(&p).get_type(), p.get_type());
        }
    }

    // =====================================================================
    // find_in_stateful / has_state_props / prop_types_for_state
    // =====================================================================

    fn sorted_stateful_fixture() -> Vec<StatefulCssProperty> {
        let mut v = vec![
            stateful(PseudoStateType::Normal, width_px(1.0)),
            stateful(
                PseudoStateType::Normal,
                CssProperty::const_none(CssPropertyType::Display),
            ),
            stateful(PseudoStateType::Hover, width_px(2.0)),
        ];
        // The lookup helpers require (state, prop_type) sort order.
        v.sort_by_key(|p| (p.state, p.prop_type));
        v
    }

    #[test]
    fn find_in_stateful_on_an_empty_slice_is_none() {
        assert!(CssPropertyCache::find_in_stateful(
            &[],
            PseudoStateType::Normal,
            &CssPropertyType::Width
        )
        .is_none());
    }

    #[test]
    fn find_in_stateful_is_keyed_on_both_state_and_prop_type() {
        let v = sorted_stateful_fixture();

        let normal_width = CssPropertyCache::find_in_stateful(
            &v,
            PseudoStateType::Normal,
            &CssPropertyType::Width,
        )
        .expect("normal width present");
        assert_eq!(normal_width.get_type(), CssPropertyType::Width);

        let hover_width =
            CssPropertyCache::find_in_stateful(&v, PseudoStateType::Hover, &CssPropertyType::Width)
                .expect("hover width present");
        // same prop type, different state => a different entry
        assert_ne!(normal_width, hover_width);

        // present prop type, absent state
        assert!(CssPropertyCache::find_in_stateful(
            &v,
            PseudoStateType::Focus,
            &CssPropertyType::Width
        )
        .is_none());
        // present state, absent prop type
        assert!(CssPropertyCache::find_in_stateful(
            &v,
            PseudoStateType::Hover,
            &CssPropertyType::Display
        )
        .is_none());
    }

    #[test]
    fn has_state_props_true_false_and_edges() {
        let v = sorted_stateful_fixture();
        assert!(CssPropertyCache::has_state_props(
            &v,
            PseudoStateType::Normal
        ));
        assert!(CssPropertyCache::has_state_props(
            &v,
            PseudoStateType::Hover
        ));
        assert!(!CssPropertyCache::has_state_props(
            &v,
            PseudoStateType::Focus
        ));
        // empty slice: deterministic false, no partition_point OOB read
        assert!(!CssPropertyCache::has_state_props(
            &[],
            PseudoStateType::Normal
        ));
    }

    #[test]
    fn prop_types_for_state_filters_by_state() {
        let v = sorted_stateful_fixture();

        let mut normal: Vec<CssPropertyType> =
            CssPropertyCache::prop_types_for_state(&v, PseudoStateType::Normal)
                .copied()
                .collect();
        normal.sort_unstable();
        assert_eq!(normal.len(), 2);
        assert!(normal.contains(&CssPropertyType::Width));
        assert!(normal.contains(&CssPropertyType::Display));

        let hover: Vec<CssPropertyType> =
            CssPropertyCache::prop_types_for_state(&v, PseudoStateType::Hover)
                .copied()
                .collect();
        assert_eq!(hover, vec![CssPropertyType::Width]);

        assert_eq!(
            CssPropertyCache::prop_types_for_state(&v, PseudoStateType::Active).count(),
            0
        );
        assert_eq!(
            CssPropertyCache::prop_types_for_state(&[], PseudoStateType::Normal).count(),
            0
        );
    }

    // =====================================================================
    // font-size resolution (numeric)
    // =====================================================================

    #[test]
    fn resolve_font_size_to_pixels_converts_absolute_units() {
        let px =
            CssPropertyCache::resolve_font_size_to_pixels(&font_size(PixelValue::px(20.0)), 10.0);
        let (metric, n) = font_size_parts(&px).unwrap();
        assert_eq!(metric, SizeMetric::Px);
        assert!(close(n, 20.0));

        let pt =
            CssPropertyCache::resolve_font_size_to_pixels(&font_size(PixelValue::pt(12.0)), 10.0);
        assert!(close(font_size_parts(&pt).unwrap().1, 12.0 * PT_TO_PX));
    }

    #[test]
    fn resolve_font_size_to_pixels_em_scales_by_reference_but_rem_does_not() {
        let em =
            CssPropertyCache::resolve_font_size_to_pixels(&font_size(PixelValue::em(2.0)), 10.0);
        assert!(close(font_size_parts(&em).unwrap().1, 20.0));

        // rem deliberately ignores the reference and uses DEFAULT_FONT_SIZE (16).
        let rem =
            CssPropertyCache::resolve_font_size_to_pixels(&font_size(PixelValue::rem(2.0)), 10.0);
        assert!(close(font_size_parts(&rem).unwrap().1, 32.0));

        let pct = CssPropertyCache::resolve_font_size_to_pixels(
            &font_size(PixelValue::percent(50.0)),
            10.0,
        );
        assert!(close(font_size_parts(&pct).unwrap().1, 5.0));
    }

    #[test]
    fn resolve_font_size_to_pixels_with_nan_and_infinite_references() {
        // NaN * anything => NaN => saturates to 0 in the fixed-point encoding.
        let nan = CssPropertyCache::resolve_font_size_to_pixels(
            &font_size(PixelValue::em(2.0)),
            f32::NAN,
        );
        let (metric, n) = font_size_parts(&nan).unwrap();
        assert_eq!(metric, SizeMetric::Px);
        assert_eq!(n, 0.0, "NaN must not escape into the cascade");

        let inf = CssPropertyCache::resolve_font_size_to_pixels(
            &font_size(PixelValue::em(2.0)),
            f32::INFINITY,
        );
        let n = font_size_parts(&inf).unwrap().1;
        assert!(n.is_finite() && n > 0.0, "saturated, got {n}");

        let zero =
            CssPropertyCache::resolve_font_size_to_pixels(&font_size(PixelValue::em(2.0)), 0.0);
        assert_eq!(font_size_parts(&zero).unwrap().1, 0.0);

        let neg =
            CssPropertyCache::resolve_font_size_to_pixels(&font_size(PixelValue::em(2.0)), -10.0);
        assert!(close(font_size_parts(&neg).unwrap().1, -20.0));
    }

    #[test]
    fn resolve_font_size_to_pixels_passes_through_unresolvable_inputs() {
        // viewport units need a viewport => returned unchanged
        let vw = font_size(PixelValue::from_metric(SizeMetric::Vw, 10.0));
        assert_eq!(CssPropertyCache::resolve_font_size_to_pixels(&vw, 16.0), vw);

        // a non-font-size property is returned verbatim
        let w = width_px(10.0);
        assert_eq!(CssPropertyCache::resolve_font_size_to_pixels(&w, 16.0), w);

        // a keyword (non-Exact) font-size has no PixelValue to convert
        let inherit = CssProperty::FontSize(CssPropertyValue::Inherit);
        assert_eq!(
            CssPropertyCache::resolve_font_size_to_pixels(&inherit, 16.0),
            inherit
        );
    }

    #[test]
    fn has_relative_font_size_unit_true_false_and_edges() {
        assert!(CssPropertyCache::has_relative_font_size_unit(&font_size(
            PixelValue::em(1.0)
        )));
        assert!(CssPropertyCache::has_relative_font_size_unit(&font_size(
            PixelValue::rem(1.0)
        )));
        assert!(CssPropertyCache::has_relative_font_size_unit(&font_size(
            PixelValue::percent(100.0)
        )));

        assert!(!CssPropertyCache::has_relative_font_size_unit(&font_size(
            PixelValue::px(16.0)
        )));
        assert!(!CssPropertyCache::has_relative_font_size_unit(&font_size(
            PixelValue::pt(12.0)
        )));
        // keyword font-size and non-font-size properties are not "relative"
        assert!(!CssPropertyCache::has_relative_font_size_unit(
            &CssProperty::FontSize(CssPropertyValue::Auto)
        ));
        assert!(!CssPropertyCache::has_relative_font_size_unit(&width_px(
            1.0
        )));
    }

    // =====================================================================
    // resolve_property_dependency
    // =====================================================================

    #[test]
    fn resolve_property_dependency_scales_relative_targets_by_an_absolute_reference() {
        let reference = font_size(PixelValue::px(10.0));

        let em = CssPropertyCache::resolve_property_dependency(
            &font_size(PixelValue::em(2.0)),
            &reference,
        )
        .expect("em resolves against an absolute reference");
        assert!(close(font_size_parts(&em).unwrap().1, 20.0));

        let pct = CssPropertyCache::resolve_property_dependency(
            &font_size(PixelValue::percent(50.0)),
            &reference,
        )
        .expect("percent resolves");
        assert!(close(font_size_parts(&pct).unwrap().1, 5.0));

        // The reference itself may be in any absolute unit.
        let pt_ref = font_size(PixelValue::pt(10.0));
        let em2 =
            CssPropertyCache::resolve_property_dependency(&font_size(PixelValue::em(2.0)), &pt_ref)
                .expect("pt reference is absolute");
        assert!(close(
            font_size_parts(&em2).unwrap().1,
            2.0 * 10.0 * PT_TO_PX
        ));
    }

    #[test]
    fn resolve_property_dependency_rewrites_the_target_variant_in_place() {
        let reference = font_size(PixelValue::px(10.0));
        let padding = CssProperty::PaddingLeft(CssPropertyValue::Exact(LayoutPaddingLeft {
            inner: PixelValue::em(3.0),
        }));
        let out = CssPropertyCache::resolve_property_dependency(&padding, &reference)
            .expect("padding is a supported target");
        match out {
            CssProperty::PaddingLeft(v) => {
                let inner = v.get_property().unwrap().inner;
                assert_eq!(inner.metric, SizeMetric::Px);
                assert!(close(inner.number.get(), 30.0));
            }
            other => panic!("variant must be preserved, got {other:?}"),
        }
    }

    #[test]
    fn resolve_property_dependency_returns_none_for_unresolvable_inputs() {
        let abs = font_size(PixelValue::px(10.0));

        // a relative reference cannot anchor anything
        assert!(CssPropertyCache::resolve_property_dependency(
            &font_size(PixelValue::em(2.0)),
            &font_size(PixelValue::em(2.0))
        )
        .is_none());
        // viewport-unit target needs a viewport
        assert!(CssPropertyCache::resolve_property_dependency(
            &font_size(PixelValue::from_metric(SizeMetric::Vh, 5.0)),
            &abs
        )
        .is_none());
        // unsupported target type (no PixelValue to extract)
        assert!(CssPropertyCache::resolve_property_dependency(&width_px(5.0), &abs).is_none());
        // unsupported reference type
        assert!(CssPropertyCache::resolve_property_dependency(
            &font_size(PixelValue::em(2.0)),
            &width_px(5.0)
        )
        .is_none());
        // keyword (non-Exact) target
        assert!(CssPropertyCache::resolve_property_dependency(
            &CssProperty::FontSize(CssPropertyValue::Inherit),
            &abs
        )
        .is_none());
    }

    // =====================================================================
    // should_apply_cascaded
    // =====================================================================

    #[test]
    fn should_apply_cascaded_respects_origin_and_relative_font_sizes() {
        let own = |p: CssProperty| {
            vec![(
                p.get_type(),
                CssPropertyWithOrigin {
                    property: p,
                    origin: CssPropertyOrigin::Own,
                },
            )]
        };
        let inherited = |p: CssProperty| {
            vec![(
                p.get_type(),
                CssPropertyWithOrigin {
                    property: p,
                    origin: CssPropertyOrigin::Inherited,
                },
            )]
        };

        // nothing computed yet => apply
        assert!(CssPropertyCache::should_apply_cascaded(
            &[],
            CssPropertyType::Width,
            &width_px(1.0)
        ));

        // the node already set it itself => the UA/cascaded value must not win
        assert!(!CssPropertyCache::should_apply_cascaded(
            &own(width_px(2.0)),
            CssPropertyType::Width,
            &width_px(1.0)
        ));

        // an inherited value is weaker than a cascaded one => apply
        assert!(CssPropertyCache::should_apply_cascaded(
            &inherited(width_px(2.0)),
            CssPropertyType::Width,
            &width_px(1.0)
        ));

        // A cascaded (UA/author) font-size — relative OR absolute — overrides an
        // inherited value: it is the node's own declared size (e.g. <h1>'s UA
        // `font-size: 2em`), and `resolve_font_size_property` resolves the `em`
        // against the parent's size, so there is no double-scaling.
        let inherited_fs = inherited(font_size(PixelValue::px(20.0)));
        assert!(CssPropertyCache::should_apply_cascaded(
            &inherited_fs,
            CssPropertyType::FontSize,
            &font_size(PixelValue::em(2.0))
        ));
        assert!(CssPropertyCache::should_apply_cascaded(
            &inherited_fs,
            CssPropertyType::FontSize,
            &font_size(PixelValue::px(12.0))
        ));
    }

    // =====================================================================
    // get_property / get_property_slow (cascade layering)
    // =====================================================================

    #[test]
    fn get_property_finds_an_inline_normal_property() {
        let c = CssPropertyCache::empty(1);
        let nd = div_with(vec![width_px(100.0)]);
        let got = c
            .get_property(&nd, &n0(), &normal(), &CssPropertyType::Width)
            .expect("inline width");
        assert_eq!(*got, width_px(100.0));
    }

    #[test]
    fn get_property_ignores_pseudo_state_props_unless_the_state_is_active() {
        let c = CssPropertyCache::empty(1);
        let nd = div_with_pseudo(vec![width_px(100.0)], PseudoStateType::Hover);

        assert!(
            c.get_property(&nd, &n0(), &normal(), &CssPropertyType::Width)
                .is_none(),
            ":hover width must not leak into the Normal state"
        );

        let hovered = StyledNodeState {
            hover: true,
            ..StyledNodeState::default()
        };
        assert_eq!(
            c.get_property(&nd, &n0(), &hovered, &CssPropertyType::Width),
            Some(&width_px(100.0))
        );
    }

    #[test]
    fn get_property_user_override_beats_inline_and_stylesheet() {
        let mut c = CssPropertyCache::empty(1);
        c.user_overridden_properties
            .push(vec![(CssPropertyType::Width, width_px(1.0))]);
        c.css_props
            .push_to(0, stateful(PseudoStateType::Normal, width_px(2.0)));
        c.css_props
            .sort_each_and_flatten(|p| (p.state, p.prop_type));

        let nd = div_with(vec![width_px(3.0)]);
        assert_eq!(
            c.get_property(&nd, &n0(), &normal(), &CssPropertyType::Width),
            Some(&width_px(1.0)),
            "user override is the top cascade layer"
        );
    }

    #[test]
    fn get_property_falls_back_through_stylesheet_global_cascaded_then_ua() {
        let nd = NodeData::create_div();

        // stylesheet layer
        let mut c = CssPropertyCache::empty(1);
        c.css_props
            .push_to(0, stateful(PseudoStateType::Normal, width_px(2.0)));
        c.css_props
            .sort_each_and_flatten(|p| (p.state, p.prop_type));
        assert_eq!(
            c.get_property(&nd, &n0(), &normal(), &CssPropertyType::Width),
            Some(&width_px(2.0))
        );

        // `*` global layer (below per-node rules)
        let mut c = CssPropertyCache::empty(1);
        c.global_css_props.push(width_px(4.0));
        assert_eq!(
            c.get_property(&nd, &n0(), &normal(), &CssPropertyType::Width),
            Some(&width_px(4.0))
        );

        // cascaded (inherited/UA) layer
        let mut c = CssPropertyCache::empty(1);
        c.cascaded_props
            .push_to(0, stateful(PseudoStateType::Normal, width_px(5.0)));
        c.cascaded_props
            .sort_each_and_flatten(|p| (p.state, p.prop_type));
        assert_eq!(
            c.get_property(&nd, &n0(), &normal(), &CssPropertyType::Width),
            Some(&width_px(5.0))
        );

        // UA fallback: a <div> has no UA width, but it does have `display: block`
        let c = CssPropertyCache::empty(1);
        assert!(c
            .get_property(&nd, &n0(), &normal(), &CssPropertyType::Width)
            .is_none());
        assert!(c
            .get_property(&nd, &n0(), &normal(), &CssPropertyType::Display)
            .is_some());
    }

    #[test]
    fn get_property_on_an_out_of_range_node_id_falls_through_to_ua_css() {
        let c = CssPropertyCache::empty(0);
        let nd = NodeData::create_div();
        let far = NodeId::new(usize::MAX / 2);

        assert!(c
            .get_property(&nd, &far, &normal(), &CssPropertyType::Width)
            .is_none());
        assert!(
            c.get_property(&nd, &far, &normal(), &CssPropertyType::Display)
                .is_some(),
            "UA CSS is node-type-keyed, not index-keyed"
        );
    }

    #[test]
    fn get_property_with_context_matches_pseudo_state_conditions() {
        let c = CssPropertyCache::empty(1);
        let nd = div_with_pseudo(vec![width_px(100.0)], PseudoStateType::Hover);

        let plain = DynamicSelectorContext::default();
        assert!(c
            .get_property_with_context(&nd, &n0(), &plain, &CssPropertyType::Width)
            .is_none());

        let mut hovered = DynamicSelectorContext::default();
        hovered.pseudo_state.hover = true;
        assert_eq!(
            c.get_property_with_context(&nd, &n0(), &hovered, &CssPropertyType::Width),
            Some(&width_px(100.0))
        );
    }

    #[test]
    fn check_properties_changed_only_fires_when_a_condition_flips() {
        let plain = DynamicSelectorContext::default();
        let mut hovered = DynamicSelectorContext::default();
        hovered.pseudo_state.hover = true;

        // unconditional props never "change" between contexts
        let unconditional = div_with(vec![width_px(1.0)]);
        assert!(!CssPropertyCache::check_properties_changed(
            &unconditional,
            &plain,
            &hovered
        ));

        let conditional = div_with_pseudo(vec![width_px(1.0)], PseudoStateType::Hover);
        assert!(CssPropertyCache::check_properties_changed(
            &conditional,
            &plain,
            &hovered
        ));
        assert!(
            !CssPropertyCache::check_properties_changed(&conditional, &plain, &plain),
            "identical contexts can never differ"
        );

        // a node with no inline style at all
        assert!(!CssPropertyCache::check_properties_changed(
            &NodeData::create_div(),
            &plain,
            &hovered
        ));
    }

    #[test]
    fn check_layout_properties_changed_ignores_non_layout_properties() {
        let plain = DynamicSelectorContext::default();
        let mut hovered = DynamicSelectorContext::default();
        hovered.pseudo_state.hover = true;

        let layout = div_with_pseudo(vec![width_px(1.0)], PseudoStateType::Hover);
        assert!(CssPropertyCache::check_layout_properties_changed(
            &layout, &plain, &hovered
        ));
        assert!(CssPropertyType::Width.can_trigger_relayout());

        // A paint-only property flipping must not force a relayout.
        let paint = div_with_pseudo(
            vec![CssProperty::const_none(CssPropertyType::BackgroundContent)],
            PseudoStateType::Hover,
        );
        assert!(!CssPropertyType::BackgroundContent.can_trigger_relayout());
        assert!(!CssPropertyCache::check_layout_properties_changed(
            &paint, &plain, &hovered
        ));
        // ...though the generic check still sees it
        assert!(CssPropertyCache::check_properties_changed(
            &paint, &plain, &hovered
        ));
    }

    // =====================================================================
    // grid-gap / scrollbar getters
    // =====================================================================

    #[test]
    fn grid_gap_and_scrollbar_getters_are_none_on_a_bare_div() {
        let c = CssPropertyCache::empty(1);
        let nd = NodeData::create_div();
        assert!(c.get_grid_gap(&nd, &n0(), &normal()).is_none());
        assert!(c.get_scrollbar_track(&nd, &n0(), &normal()).is_none());
        assert!(c.get_scrollbar_thumb(&nd, &n0(), &normal()).is_none());
        assert!(c.get_scrollbar_button(&nd, &n0(), &normal()).is_none());
        assert!(c.get_scrollbar_corner(&nd, &n0(), &normal()).is_none());
        assert!(c.get_scrollbar_resizer(&nd, &n0(), &normal()).is_none());

        // and on an out-of-range node id
        let far = NodeId::new(4_242);
        assert!(c.get_grid_gap(&nd, &far, &normal()).is_none());
        assert!(c.get_scrollbar_thumb(&nd, &far, &normal()).is_none());
    }

    // =====================================================================
    // get_computed_css_style_string
    // =====================================================================

    #[test]
    fn computed_css_style_string_serializes_set_properties() {
        let c = CssPropertyCache::empty(1);

        // A bare <div> still gets `display: block` from the UA sheet.
        let s = c.get_computed_css_style_string(&NodeData::create_div(), &n0(), &normal());
        assert!(s.contains("display:"), "got {s:?}");

        let styled = div_with(vec![width_px(100.0), font_size(PixelValue::px(12.0))]);
        let s = c.get_computed_css_style_string(&styled, &n0(), &normal());
        assert!(s.contains("width:"), "got {s:?}");
        assert!(s.contains("font-size:"), "got {s:?}");
        assert!(s.ends_with(';'), "each declaration is terminated: {s:?}");
    }

    #[test]
    fn computed_css_style_string_does_not_panic_on_an_out_of_range_node_id() {
        let c = CssPropertyCache::empty(0);
        let s = c.get_computed_css_style_string(
            &NodeData::create_div(),
            &NodeId::new(usize::MAX / 2),
            &normal(),
        );
        assert!(s.contains("display:"));
    }

    // =====================================================================
    // apply_ua_css / sort_cascaded_props / prune_compact_normal_props
    // =====================================================================

    #[test]
    fn apply_ua_css_inserts_ua_properties_into_cascaded_props() {
        let nodes = vec![NodeData::create_div()];
        let mut c = CssPropertyCache::empty(1);
        c.apply_ua_css(&nodes);

        let props = c.cascaded_props.build_get(0).expect("build phase");
        assert!(
            props
                .iter()
                .any(|p| p.prop_type == CssPropertyType::Display
                    && p.state == PseudoStateType::Normal),
            "UA `div {{ display: block }}` must land in the cascade"
        );
    }

    #[test]
    fn apply_ua_css_does_not_override_an_existing_inline_property() {
        let nodes = vec![div_with(vec![CssProperty::const_none(
            CssPropertyType::Display,
        )])];
        let mut c = CssPropertyCache::empty(1);
        c.apply_ua_css(&nodes);

        let props = c.cascaded_props.build_get(0).expect("build phase");
        assert!(
            !props
                .iter()
                .any(|p| p.prop_type == CssPropertyType::Display),
            "UA CSS is the weakest layer and must not clobber inline"
        );
    }

    #[test]
    fn apply_ua_css_on_zero_nodes_returns_early() {
        let mut c = CssPropertyCache::empty(0);
        c.apply_ua_css(&[]);
        assert_eq!(c.cascaded_props.len(), 0);
    }

    #[test]
    fn sort_cascaded_props_flattens_and_orders_by_state_then_type() {
        let mut c = CssPropertyCache::empty(1);
        c.cascaded_props
            .push_to(0, stateful(PseudoStateType::Hover, width_px(1.0)));
        c.cascaded_props.push_to(
            0,
            stateful(
                PseudoStateType::Normal,
                CssProperty::const_none(CssPropertyType::Display),
            ),
        );
        c.cascaded_props
            .push_to(0, stateful(PseudoStateType::Normal, width_px(2.0)));

        c.sort_cascaded_props();

        assert!(c.cascaded_props.is_flattened());
        let slice = c.cascaded_props.get_slice(0);
        assert_eq!(slice.len(), 3);
        let keys: Vec<_> = slice.iter().map(|p| (p.state, p.prop_type)).collect();
        let mut sorted = keys.clone();
        sorted.sort_unstable();
        assert_eq!(keys, sorted, "binary_search lookups require sort order");
    }

    #[test]
    fn prune_compact_normal_props_keeps_what_the_slow_path_still_needs() {
        let mut c = CssPropertyCache::empty(1);
        // Normal + compact-encoded + fully representable => droppable
        c.cascaded_props.push_to(
            0,
            stateful(
                PseudoStateType::Normal,
                CssProperty::const_none(CssPropertyType::Display),
            ),
        );
        // Normal + compact-encoded but SENTINEL-encoded (%) => must survive
        c.cascaded_props
            .push_to(0, stateful(PseudoStateType::Normal, width_pct(50.0)));
        // Normal + no compact encoding at all => must survive
        c.cascaded_props.push_to(
            0,
            stateful(
                PseudoStateType::Normal,
                CssProperty::const_none(CssPropertyType::BackgroundContent),
            ),
        );
        // non-Normal => always survives
        c.cascaded_props.push_to(
            0,
            stateful(
                PseudoStateType::Hover,
                CssProperty::const_none(CssPropertyType::Display),
            ),
        );

        c.prune_compact_normal_props();

        let kept: Vec<(PseudoStateType, CssPropertyType)> = c
            .cascaded_props
            .get_slice(0)
            .iter()
            .map(|p| (p.state, p.prop_type))
            .collect();

        assert!(
            !kept.contains(&(PseudoStateType::Normal, CssPropertyType::Display)),
            "the compact cache is authoritative for this one"
        );
        assert!(kept.contains(&(PseudoStateType::Normal, CssPropertyType::Width)));
        assert!(kept.contains(&(PseudoStateType::Normal, CssPropertyType::BackgroundContent)));
        assert!(kept.contains(&(PseudoStateType::Hover, CssPropertyType::Display)));
        assert_eq!(kept.len(), 3);
    }

    #[test]
    fn prune_compact_normal_props_on_an_empty_cache_does_not_panic() {
        let mut c = CssPropertyCache::empty(0);
        c.prune_compact_normal_props();
        assert_eq!(c.cascaded_props.len(), 0);

        let mut c = CssPropertyCache::empty(3);
        c.prune_compact_normal_props();
        assert_eq!(c.cascaded_props.len(), 3);
        assert!(c.cascaded_props.get_slice(0).is_empty());
    }

    // =====================================================================
    // compute_inherited_values
    // =====================================================================

    /// `[root, child]`, child's parent = root (the hierarchy uses 1-based ids).
    fn two_node_hierarchy() -> Vec<NodeHierarchyItem> {
        vec![
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
                last_child: 0,
            },
        ]
    }

    #[test]
    fn compute_inherited_values_propagates_font_size_to_children() {
        let hierarchy = two_node_hierarchy();
        assert_eq!(hierarchy[1].parent_id(), Some(NodeId::new(0)));

        let nodes = vec![
            div_with(vec![font_size(PixelValue::px(20.0))]),
            NodeData::create_div(),
        ];
        let mut c = CssPropertyCache::empty(2);
        let changed = c.compute_inherited_values(&hierarchy, &nodes);

        assert_eq!(c.computed_values.len(), 2);
        assert_eq!(changed.len(), 2, "both nodes gained a computed value");

        let (t, v) = &c.computed_values[1][0];
        assert_eq!(*t, CssPropertyType::FontSize);
        assert_eq!(v.origin, CssPropertyOrigin::Inherited);
        assert!(close(font_size_parts(&v.property).unwrap().1, 20.0));

        // the parent's own value keeps the Own origin
        assert_eq!(c.computed_values[0][0].1.origin, CssPropertyOrigin::Own);
    }

    #[test]
    fn compute_inherited_values_resolves_a_child_em_against_the_parent_px() {
        let hierarchy = two_node_hierarchy();
        let nodes = vec![
            div_with(vec![font_size(PixelValue::px(20.0))]),
            div_with(vec![font_size(PixelValue::em(2.0))]),
        ];
        let mut c = CssPropertyCache::empty(2);
        c.compute_inherited_values(&hierarchy, &nodes);

        let (t, v) = &c.computed_values[1][0];
        assert_eq!(*t, CssPropertyType::FontSize);
        assert_eq!(v.origin, CssPropertyOrigin::Own);
        let (metric, n) = font_size_parts(&v.property).unwrap();
        assert_eq!(metric, SizeMetric::Px, "resolved to absolute px");
        assert!(close(n, 40.0), "2em of the parent's 20px, got {n}");
    }

    #[test]
    fn compute_inherited_values_is_idempotent_on_a_second_run() {
        let hierarchy = two_node_hierarchy();
        let nodes = vec![
            div_with(vec![font_size(PixelValue::px(20.0))]),
            NodeData::create_div(),
        ];
        let mut c = CssPropertyCache::empty(2);
        assert_eq!(c.compute_inherited_values(&hierarchy, &nodes).len(), 2);
        assert!(
            c.compute_inherited_values(&hierarchy, &nodes).is_empty(),
            "nothing changed the second time around"
        );
    }

    #[test]
    fn compute_inherited_values_on_an_empty_tree_does_not_panic() {
        let mut c = CssPropertyCache::empty(0);
        assert!(c.compute_inherited_values(&[], &[]).is_empty());
        assert!(c.computed_values.is_empty());
    }

    // =====================================================================
    // restyle / generate_tag_ids
    // =====================================================================

    fn one_node_scaffold() -> (NodeHierarchyItemVec, NodeDataContainer<CascadeInfo>) {
        (
            vec![NodeHierarchyItem::zeroed()].into(),
            NodeDataContainer::new(vec![CascadeInfo {
                index_in_parent: 0,
                is_last_child: true,
            }]),
        )
    }

    #[test]
    fn restyle_with_an_empty_stylesheet_flattens_and_yields_no_tags() {
        let (hierarchy, cascade) = one_node_scaffold();
        let nodes = NodeDataContainer::new(vec![NodeData::create_div()]);
        let non_leaf: ParentWithNodeDepthVec = Vec::new().into();
        let mut css = Css::empty();

        let mut c = CssPropertyCache::empty(1);
        let tags = c.restyle(
            &mut css,
            &nodes.as_ref(),
            &hierarchy,
            &non_leaf,
            &cascade.as_ref(),
        );

        assert!(tags.is_empty(), "a plain div needs no hit-test tag");
        assert!(
            c.css_props.is_flattened(),
            "restyle must leave css_props in read phase"
        );
        assert!(c.resolved_font_sizes_px.get().is_none());
    }

    #[test]
    fn generate_tag_ids_skips_inert_nodes_and_tags_interactive_ones() {
        let (hierarchy, _) = one_node_scaffold();

        let inert = NodeDataContainer::new(vec![NodeData::create_div()]);
        let c = CssPropertyCache::empty(1);
        assert!(c.generate_tag_ids(&inert.as_ref(), &hierarchy).is_empty());

        // an inline :hover rule makes the node hit-testable
        let hoverable = NodeDataContainer::new(vec![div_with_pseudo(
            vec![width_px(1.0)],
            PseudoStateType::Hover,
        )]);
        let tags = c.generate_tag_ids(&hoverable.as_ref(), &hierarchy);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].node_id.into_crate_internal(), Some(NodeId::new(0)));
    }

    #[test]
    fn generate_tag_ids_tags_a_node_with_a_cursor_declaration() {
        let (hierarchy, _) = one_node_scaffold();
        let nodes = NodeDataContainer::new(vec![div_with(vec![CssProperty::const_none(
            CssPropertyType::Cursor,
        )])]);
        let c = CssPropertyCache::empty(1);
        assert_eq!(c.generate_tag_ids(&nodes.as_ref(), &hierarchy).len(), 1);
    }

    #[test]
    fn generate_tag_ids_on_an_empty_dom_yields_nothing() {
        let nodes: NodeDataContainer<NodeData> = NodeDataContainer::new(Vec::new());
        let hierarchy: NodeHierarchyItemVec = Vec::new().into();
        let c = CssPropertyCache::empty(0);
        assert!(c.generate_tag_ids(&nodes.as_ref(), &hierarchy).is_empty());
    }

    // =====================================================================
    // std-gated profiling helpers
    // =====================================================================

    #[cfg(feature = "std")]
    #[test]
    fn css_prop_type_label_is_interned_and_distinct_per_variant() {
        let a = CssPropertyCache::css_prop_type_label(&CssPropertyType::Width);
        let b = CssPropertyCache::css_prop_type_label(&CssPropertyType::Width);
        assert!(!a.is_empty());
        assert_eq!(
            a.as_ptr(),
            b.as_ptr(),
            "the label table must leak at most one &'static str per variant"
        );

        let other = CssPropertyCache::css_prop_type_label(&CssPropertyType::Height);
        assert_ne!(a, other);
    }

    #[cfg(feature = "std")]
    #[test]
    fn drain_css_prop_counts_is_sorted_descending_and_drains() {
        // The counter is thread-local and only records when AZ_PROP_COUNT=1, so
        // the contract to pin here is "never panics, and drains".
        let first = drain_css_prop_counts();
        for w in first.windows(2) {
            assert!(w[0].1 >= w[1].1, "counts must be sorted descending");
        }
        assert!(
            drain_css_prop_counts().is_empty(),
            "a drained counter comes back empty"
        );
    }
}
