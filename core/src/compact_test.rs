#[allow(unused_imports)]
pub use super::*;
#[cfg(test)]
mod audit_tests {
    use super::resolve_font_size_to_px;
    use crate::dom::NodeId;
    use azul_css::compact_cache::{
        decode_pixel_value_u32, encode_pixel_value_u32, CompactNodeProps,
    };
    use azul_css::props::basic::pixel::PixelValue;

    // Happy path: an `em` font-size resolves against a valid (pre-order) parent.
    #[test]
    fn resolve_font_size_em_from_parent() {
        let mut dims = vec![CompactNodeProps::default(); 2];
        dims[0].font_size = encode_pixel_value_u32(&PixelValue::px(20.0));
        dims[1].font_size = encode_pixel_value_u32(&PixelValue::em(2.0));
        resolve_font_size_to_px(&mut dims, 1, Some(NodeId::new(0)));
        let pv = decode_pixel_value_u32(dims[1].font_size).unwrap();
        assert!(
            (pv.number.get() - 40.0).abs() < 0.01,
            "got {}",
            pv.number.get()
        );
    }

    // Root `em` (no parent) uses the 16px CSS initial value.
    #[test]
    fn resolve_font_size_root_em_uses_default() {
        let mut dims = vec![CompactNodeProps::default()];
        dims[0].font_size = encode_pixel_value_u32(&PixelValue::em(2.0));
        resolve_font_size_to_px(&mut dims, 0, None);
        let pv = decode_pixel_value_u32(dims[0].font_size).unwrap();
        assert!(
            (pv.number.get() - 32.0).abs() < 0.01,
            "got {}",
            pv.number.get()
        );
    }

    // A `rem` value reads the root (index 0) via the `.first()` guard without
    // panicking (previously indexed `tier2_dims[0]` directly).
    #[test]
    fn resolve_font_size_rem_reads_root() {
        let mut dims = vec![CompactNodeProps::default(); 2];
        dims[0].font_size = encode_pixel_value_u32(&PixelValue::px(10.0)); // root
        dims[1].font_size = encode_pixel_value_u32(&PixelValue::rem(3.0)); // child rem
        resolve_font_size_to_px(&mut dims, 1, Some(NodeId::new(0)));
        let pv = decode_pixel_value_u32(dims[1].font_size).unwrap();
        assert!(
            (pv.number.get() - 30.0).abs() < 0.01,
            "got {}",
            pv.number.get()
        );
    }
}

#[cfg(test)]
#[allow(
    clippy::float_cmp,
    clippy::unreadable_literal,
    clippy::too_many_lines,
    clippy::cast_lossless
)]
mod autotest_generated {
    use super::*;

    use alloc::collections::BTreeMap;

    use crate::dom::NodeType;
    use crate::styled_dom::NodeHierarchyItem;
    use azul_css::props::basic::color::ColorU;
    use azul_css::props::basic::font::{StyleFontFamily, StyleFontFamilyVec};
    use azul_css::props::basic::length::{FloatValue, PercentageValue};
    use azul_css::props::basic::pixel::PixelValue;
    use azul_css::props::layout::dimensions::LayoutMinWidth;
    use azul_css::props::layout::display::LayoutDisplay;
    use azul_css::props::layout::flex::{LayoutFlexGrow, LayoutFlexShrink};
    use azul_css::props::layout::grid::{GridLine, GridPlacement, LayoutGap, NamedGridLine};
    use azul_css::props::layout::overflow::StyleScrollbarGutter;
    use azul_css::props::layout::position::LayoutPosition;
    use azul_css::props::layout::spacing::{LayoutMarginTop, LayoutPaddingTop};
    use azul_css::props::layout::table::StyleBorderCollapse;
    use azul_css::props::style::border::{BorderStyle, StyleBorderTopStyle};
    use azul_css::props::style::effects::StyleOpacity;
    use azul_css::props::style::text::{
        StyleLineHeight, StyleTextColor, StyleTextDecoration, StyleTextIndent,
    };

    // -------------------------------------------------------------------------
    // Fixtures
    // -------------------------------------------------------------------------

    /// The four compact output slots + the font reverse-map, as one value, so a
    /// test can snapshot "everything the writer could have touched".
    struct Sink {
        tier1: u64,
        dims: CompactNodeProps,
        cold: CompactNodePropsCold,
        text: CompactTextProps,
        fonts: BTreeMap<u64, StyleFontFamilyVec>,
    }

    impl Sink {
        fn new() -> Self {
            Self {
                tier1: 0,
                dims: CompactNodeProps::default(),
                cold: CompactNodePropsCold::default(),
                text: CompactTextProps::default(),
                fonts: BTreeMap::new(),
            }
        }

        fn apply(&mut self, prop: &CssProperty) {
            apply_css_property_to_compact(
                prop,
                &mut self.tier1,
                &mut self.dims,
                &mut self.cold,
                &mut self.text,
                &mut self.fonts,
            );
        }

        fn ua(&mut self, node_type: &NodeType) {
            apply_ua_css_to_compact(
                node_type,
                &mut self.tier1,
                &mut self.dims,
                &mut self.cold,
                &mut self.text,
                &mut self.fonts,
            );
        }

        fn snapshot(
            &self,
        ) -> (
            u64,
            CompactNodeProps,
            CompactNodePropsCold,
            CompactTextProps,
        ) {
            (self.tier1, self.dims, self.cold, self.text)
        }
    }

    fn div_nodes(n: usize) -> Vec<NodeData> {
        (0..n)
            .map(|_| NodeData::create_node(NodeType::Div))
            .collect()
    }

    /// Pre-order chain: node 0 is the root, node `i` is the child of node `i-1`.
    /// `NodeHierarchyItem` uses 1-based encoding (0 = None, n = `NodeId(n-1)`).
    fn linear_hierarchy(n: usize) -> Vec<NodeHierarchyItem> {
        (0..n)
            .map(|i| NodeHierarchyItem {
                parent: i, // i == 0 -> None; i > 0 -> NodeId(i-1)
                previous_sibling: 0,
                next_sibling: 0,
                last_child: if i + 1 < n { i + 2 } else { 0 },
            })
            .collect()
    }

    fn padding(px: f32) -> CssPropertyValue<LayoutPaddingTop> {
        CssPropertyValue::Exact(LayoutPaddingTop {
            inner: PixelValue::px(px),
        })
    }

    // -------------------------------------------------------------------------
    // encode_grid_line
    // -------------------------------------------------------------------------

    #[test]
    fn grid_line_auto_and_named_map_to_their_sentinels() {
        assert_eq!(encode_grid_line(&GridLine::Auto), I16_AUTO);
        let named = GridLine::Named(NamedGridLine {
            grid_line_name: "sidebar".into(),
            span_count: 0,
        });
        assert_eq!(encode_grid_line(&named), I16_SENTINEL);
    }

    #[test]
    fn grid_line_number_boundaries_saturate_instead_of_truncating() {
        assert_eq!(encode_grid_line(&GridLine::Line(0)), 0);
        assert_eq!(encode_grid_line(&GridLine::Line(1)), 1);
        assert_eq!(encode_grid_line(&GridLine::Line(-1)), -1);
        assert_eq!(encode_grid_line(&GridLine::Line(32_000)), 32_000);
        assert_eq!(encode_grid_line(&GridLine::Line(-32_000)), -32_000);
        // One past the guarded range: must become the sentinel, never a wrapped i16.
        assert_eq!(encode_grid_line(&GridLine::Line(32_001)), I16_SENTINEL);
        assert_eq!(encode_grid_line(&GridLine::Line(-32_001)), I16_SENTINEL);
        assert_eq!(encode_grid_line(&GridLine::Line(i32::MAX)), I16_SENTINEL);
        assert_eq!(encode_grid_line(&GridLine::Line(i32::MIN)), I16_SENTINEL);
    }

    #[test]
    fn grid_line_span_boundaries_and_nonsense_spans() {
        assert_eq!(encode_grid_line(&GridLine::Span(1)), -1);
        assert_eq!(encode_grid_line(&GridLine::Span(32_000)), -32_000);
        // `span 0` / negative spans are not representable -> sentinel, NOT 0 (which
        // would silently mean "grid line 0").
        assert_eq!(encode_grid_line(&GridLine::Span(0)), I16_SENTINEL);
        assert_eq!(encode_grid_line(&GridLine::Span(-1)), I16_SENTINEL);
        assert_eq!(encode_grid_line(&GridLine::Span(32_001)), I16_SENTINEL);
        assert_eq!(encode_grid_line(&GridLine::Span(i32::MAX)), I16_SENTINEL);
        assert_eq!(encode_grid_line(&GridLine::Span(i32::MIN)), I16_SENTINEL);
    }

    #[test]
    fn grid_line_in_range_values_never_alias_the_sentinel_band() {
        // A real line number that lands on >= I16_SENTINEL_THRESHOLD would decode
        // as "auto" / "overflow" and move the item to a different grid cell.
        for n in [-32_000i32, -1_000, -1, 0, 1, 1_000, 32_000] {
            let e = encode_grid_line(&GridLine::Line(n));
            assert!(
                e < I16_SENTINEL_THRESHOLD,
                "Line({n}) encoded into the sentinel band as {e}"
            );
        }
        for n in [1i32, 2, 1_000, 32_000] {
            let e = encode_grid_line(&GridLine::Span(n));
            assert!(e < 0, "Span({n}) must encode as a negative value, got {e}");
            assert!(
                e < I16_SENTINEL_THRESHOLD,
                "Span({n}) encoded into the sentinel band as {e}"
            );
        }
    }

    // -------------------------------------------------------------------------
    // encode_layout_width / encode_layout_height
    // -------------------------------------------------------------------------

    #[test]
    fn layout_width_keywords_map_to_distinct_sentinels() {
        let auto: CssPropertyValue<LayoutWidth> = CssPropertyValue::Auto;
        let none: CssPropertyValue<LayoutWidth> = CssPropertyValue::None;
        let initial: CssPropertyValue<LayoutWidth> = CssPropertyValue::Initial;
        let inherit: CssPropertyValue<LayoutWidth> = CssPropertyValue::Inherit;
        assert_eq!(encode_layout_width(&auto), U32_AUTO);
        assert_eq!(encode_layout_width(&none), U32_NONE);
        assert_eq!(encode_layout_width(&initial), U32_INITIAL);
        assert_eq!(encode_layout_width(&inherit), U32_INHERIT);
    }

    #[test]
    fn layout_width_revert_and_unset_fall_back_to_the_overflow_sentinel() {
        // `revert` / `unset` have no compact slot. They must land on U32_SENTINEL
        // (= "ask the slow path"), never on a *semantic* sentinel like AUTO.
        let revert: CssPropertyValue<LayoutWidth> = CssPropertyValue::Revert;
        let unset: CssPropertyValue<LayoutWidth> = CssPropertyValue::Unset;
        assert_eq!(encode_layout_width(&revert), U32_SENTINEL);
        assert_eq!(encode_layout_width(&unset), U32_SENTINEL);
        assert_eq!(encode_layout_height(&revert), U32_SENTINEL);
        assert_eq!(encode_layout_height(&unset), U32_SENTINEL);
    }

    #[test]
    fn layout_width_exact_keyword_variants() {
        assert_eq!(
            encode_layout_width(&CssPropertyValue::Exact(LayoutWidth::Auto)),
            U32_AUTO
        );
        assert_eq!(
            encode_layout_width(&CssPropertyValue::Exact(LayoutWidth::MinContent)),
            U32_MIN_CONTENT
        );
        assert_eq!(
            encode_layout_width(&CssPropertyValue::Exact(LayoutWidth::MaxContent)),
            U32_MAX_CONTENT
        );
        // fit-content() is not compact-encodable -> tier 3
        assert_eq!(
            encode_layout_width(&CssPropertyValue::Exact(LayoutWidth::FitContent(
                PixelValue::px(10.0)
            ))),
            U32_SENTINEL
        );
    }

    #[test]
    fn layout_width_px_round_trips() {
        for px in [0.0f32, 0.5, 1.0, 100.0, 1234.567, -50.0] {
            let enc = encode_layout_width(&CssPropertyValue::Exact(LayoutWidth::Px(
                PixelValue::px(px),
            )));
            let dec = decode_pixel_value_u32(enc)
                .expect("an in-range px value must not encode to a sentinel");
            assert_eq!(dec.metric, SizeMetric::Px);
            assert!(
                (dec.number.get() - px).abs() < 0.002,
                "round-trip of {px}px produced {}px",
                dec.number.get()
            );
        }
    }

    #[test]
    fn layout_width_extreme_values_saturate_to_the_overflow_sentinel() {
        // Past the 28-bit fixed-point range the encoder must bail to tier 3 rather
        // than wrapping the low bits into a small (and plausible-looking) width.
        for px in [
            1.0e9f32,
            -1.0e9,
            f32::MAX,
            f32::MIN,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ] {
            let enc = encode_layout_width(&CssPropertyValue::Exact(LayoutWidth::Px(
                PixelValue::px(px),
            )));
            assert_eq!(
                enc, U32_SENTINEL,
                "width {px}px should overflow to U32_SENTINEL, got {enc:#x}"
            );
        }
    }

    #[test]
    fn layout_width_nan_degrades_to_zero_without_panicking() {
        // `NaN as isize` saturates to 0, so a NaN width becomes 0px — deterministic
        // and finite, which is what the layout solver needs.
        let enc = encode_layout_width(&CssPropertyValue::Exact(LayoutWidth::Px(PixelValue::px(
            f32::NAN,
        ))));
        let dec = decode_pixel_value_u32(enc).expect("NaN must degrade to a value, not a sentinel");
        assert!(dec.number.get().is_finite());
        assert_eq!(dec.number.get(), 0.0);
    }

    #[test]
    fn layout_height_never_diverges_from_layout_width() {
        let vals = [
            CssPropertyValue::Exact(LayoutWidth::Auto),
            CssPropertyValue::Exact(LayoutWidth::MinContent),
            CssPropertyValue::Exact(LayoutWidth::MaxContent),
            CssPropertyValue::Exact(LayoutWidth::Px(PixelValue::px(42.0))),
            CssPropertyValue::Exact(LayoutWidth::Px(PixelValue::px(1.0e9))),
            CssPropertyValue::Unset,
        ];
        for v in &vals {
            assert_eq!(encode_layout_width(v), encode_layout_height(v));
        }
    }

    // -------------------------------------------------------------------------
    // encode_pixel_prop
    // -------------------------------------------------------------------------

    #[test]
    fn pixel_prop_keywords_map_to_distinct_sentinels() {
        let auto: CssPropertyValue<LayoutMinWidth> = CssPropertyValue::Auto;
        let none: CssPropertyValue<LayoutMinWidth> = CssPropertyValue::None;
        let initial: CssPropertyValue<LayoutMinWidth> = CssPropertyValue::Initial;
        let inherit: CssPropertyValue<LayoutMinWidth> = CssPropertyValue::Inherit;
        let revert: CssPropertyValue<LayoutMinWidth> = CssPropertyValue::Revert;
        let unset: CssPropertyValue<LayoutMinWidth> = CssPropertyValue::Unset;
        assert_eq!(encode_pixel_prop(&auto), U32_AUTO);
        assert_eq!(encode_pixel_prop(&none), U32_NONE);
        assert_eq!(encode_pixel_prop(&initial), U32_INITIAL);
        assert_eq!(encode_pixel_prop(&inherit), U32_INHERIT);
        assert_eq!(encode_pixel_prop(&revert), U32_SENTINEL);
        assert_eq!(encode_pixel_prop(&unset), U32_SENTINEL);
    }

    #[test]
    fn pixel_prop_round_trips_value_and_metric() {
        for pv in [
            PixelValue::px(50.0),
            PixelValue::em(1.5),
            PixelValue::percent(80.0),
            PixelValue::pt(12.0),
            PixelValue::rem(2.0),
        ] {
            let enc = encode_pixel_prop(&CssPropertyValue::Exact(LayoutMinWidth { inner: pv }));
            let dec = decode_pixel_value_u32(enc).expect("must round-trip");
            assert_eq!(dec.metric, pv.metric, "metric lost in the round-trip");
            assert!(
                (dec.number.get() - pv.number.get()).abs() < 0.002,
                "value lost in the round-trip: {} -> {}",
                pv.number.get(),
                dec.number.get()
            );
        }
    }

    #[test]
    fn pixel_prop_overflow_saturates() {
        let enc = encode_pixel_prop(&CssPropertyValue::Exact(LayoutMinWidth {
            inner: PixelValue::px(1.0e9),
        }));
        assert_eq!(enc, U32_SENTINEL);
    }

    #[test]
    fn pixel_prop_exact_value_never_aliases_a_semantic_sentinel() {
        // INVARIANT: an `Exact` length may overflow to U32_SENTINEL (= "slow path"),
        // but must never collide with a sentinel that means something *else*
        // (auto / none / inherit / initial / min-content / max-content) — that turns
        // a length into a different keyword with no way to tell.
        //
        // `encode_pixel_value_u32` packs `value << 4 | metric`. For the raw
        // fixed-point value -1 (i.e. -0.001) the value bits are 0xFFFF_FFF0, so any
        // metric whose code is >= 9 (vh = 9, vmin = 10, vmax = 11) ORs straight into
        // the sentinel band:
        //     -0.001vh   -> 0xFFFF_FFF9 == U32_MAX_CONTENT
        //     -0.001vmin -> 0xFFFF_FFFA == U32_MIN_CONTENT
        //     -0.001vmax -> 0xFFFF_FFFB == U32_INITIAL
        for metric in [SizeMetric::Vh, SizeMetric::Vmin, SizeMetric::Vmax] {
            let pv = PixelValue::from_metric(metric, -0.001);
            let enc = encode_pixel_prop(&CssPropertyValue::Exact(LayoutMinWidth { inner: pv }));
            assert!(
                enc == U32_SENTINEL || enc < U32_SENTINEL_THRESHOLD,
                "an Exact viewport length encoded to {enc:#x}, which aliases a semantic sentinel",
            );
        }
    }

    // -------------------------------------------------------------------------
    // encode_css_pixel_as_i16 / encode_margin_i16
    // -------------------------------------------------------------------------

    #[test]
    fn css_pixel_i16_scales_by_ten() {
        assert_eq!(encode_css_pixel_as_i16(&padding(0.0)), 0);
        assert_eq!(encode_css_pixel_as_i16(&padding(10.5)), 105);
        assert_eq!(encode_css_pixel_as_i16(&padding(-10.5)), -105);
    }

    #[test]
    fn css_pixel_i16_boundaries() {
        // 3276.3px is the largest representable value (one below the sentinel band)
        assert_eq!(encode_css_pixel_as_i16(&padding(3276.3)), 32_763);
        // one tick further must saturate, NOT alias I16_INITIAL (32764)
        assert_eq!(encode_css_pixel_as_i16(&padding(3276.4)), I16_SENTINEL);
        // and the negative end
        assert_eq!(encode_css_pixel_as_i16(&padding(-3276.8)), -32_768);
        assert_eq!(encode_css_pixel_as_i16(&padding(-3276.9)), I16_SENTINEL);
    }

    #[test]
    fn css_pixel_i16_non_px_units_need_the_slow_path() {
        let em = CssPropertyValue::Exact(LayoutPaddingTop {
            inner: PixelValue::em(2.0),
        });
        let pct = CssPropertyValue::Exact(LayoutPaddingTop {
            inner: PixelValue::percent(50.0),
        });
        assert_eq!(encode_css_pixel_as_i16(&em), I16_SENTINEL);
        assert_eq!(encode_css_pixel_as_i16(&pct), I16_SENTINEL);
    }

    #[test]
    fn css_pixel_i16_keywords_are_distinguishable() {
        let auto: CssPropertyValue<LayoutPaddingTop> = CssPropertyValue::Auto;
        let initial: CssPropertyValue<LayoutPaddingTop> = CssPropertyValue::Initial;
        let inherit: CssPropertyValue<LayoutPaddingTop> = CssPropertyValue::Inherit;
        let none: CssPropertyValue<LayoutPaddingTop> = CssPropertyValue::None;
        let revert: CssPropertyValue<LayoutPaddingTop> = CssPropertyValue::Revert;
        let unset: CssPropertyValue<LayoutPaddingTop> = CssPropertyValue::Unset;
        assert_eq!(encode_css_pixel_as_i16(&auto), I16_AUTO);
        assert_eq!(encode_css_pixel_as_i16(&initial), I16_INITIAL);
        assert_eq!(encode_css_pixel_as_i16(&inherit), I16_INHERIT);
        // none / revert / unset have no dedicated slot -> generic sentinel
        assert_eq!(encode_css_pixel_as_i16(&none), I16_SENTINEL);
        assert_eq!(encode_css_pixel_as_i16(&revert), I16_SENTINEL);
        assert_eq!(encode_css_pixel_as_i16(&unset), I16_SENTINEL);
    }

    #[test]
    fn css_pixel_i16_nan_and_infinity_are_safe() {
        assert_eq!(encode_css_pixel_as_i16(&padding(f32::NAN)), 0);
        assert_eq!(
            encode_css_pixel_as_i16(&padding(f32::INFINITY)),
            I16_SENTINEL
        );
        assert_eq!(
            encode_css_pixel_as_i16(&padding(f32::NEG_INFINITY)),
            I16_SENTINEL
        );
        assert_eq!(encode_css_pixel_as_i16(&padding(f32::MAX)), I16_SENTINEL);
        assert_eq!(encode_css_pixel_as_i16(&padding(f32::MIN)), I16_SENTINEL);
    }

    #[test]
    fn css_pixel_i16_exact_value_never_aliases_a_keyword_sentinel() {
        // The i16 encoder range-checks *both* ends before narrowing, so — unlike the
        // u32 path — an Exact px value can never be mistaken for auto/inherit/initial.
        for px in [
            -3276.8f32, -100.0, -0.1, 0.0, 0.1, 100.0, 3276.3, 1.0e9, -1.0e9,
        ] {
            let e = encode_css_pixel_as_i16(&padding(px));
            assert!(
                e != I16_AUTO && e != I16_INHERIT && e != I16_INITIAL,
                "{px}px aliased a keyword sentinel ({e})"
            );
        }
    }

    #[test]
    fn margin_i16_keeps_auto_and_otherwise_matches_the_pixel_encoder() {
        let auto: CssPropertyValue<LayoutMarginTop> = CssPropertyValue::Auto;
        assert_eq!(encode_margin_i16(&auto), I16_AUTO);
        for px in [-50.0f32, 0.0, 12.5, 3276.3, 5.0e9, f32::NAN] {
            let m = CssPropertyValue::Exact(LayoutMarginTop {
                inner: PixelValue::px(px),
            });
            assert_eq!(encode_margin_i16(&m), encode_css_pixel_as_i16(&padding(px)));
        }
    }

    // -------------------------------------------------------------------------
    // encode_flex_basis
    // -------------------------------------------------------------------------

    #[test]
    fn flex_basis_all_variants() {
        assert_eq!(
            encode_flex_basis(&CssPropertyValue::Exact(LayoutFlexBasis::Auto)),
            U32_AUTO
        );
        let enc = encode_flex_basis(&CssPropertyValue::Exact(LayoutFlexBasis::Exact(
            PixelValue::px(120.0),
        )));
        let dec = decode_pixel_value_u32(enc).expect("px flex-basis must round-trip");
        assert!((dec.number.get() - 120.0).abs() < 0.002);

        assert_eq!(encode_flex_basis(&CssPropertyValue::Auto), U32_AUTO);
        assert_eq!(encode_flex_basis(&CssPropertyValue::None), U32_NONE);
        assert_eq!(encode_flex_basis(&CssPropertyValue::Initial), U32_INITIAL);
        assert_eq!(encode_flex_basis(&CssPropertyValue::Inherit), U32_INHERIT);
        assert_eq!(encode_flex_basis(&CssPropertyValue::Revert), U32_SENTINEL);
        assert_eq!(encode_flex_basis(&CssPropertyValue::Unset), U32_SENTINEL);
    }

    #[test]
    fn flex_basis_overflow_saturates() {
        assert_eq!(
            encode_flex_basis(&CssPropertyValue::Exact(LayoutFlexBasis::Exact(
                PixelValue::px(1.0e9)
            ))),
            U32_SENTINEL
        );
        assert_eq!(
            encode_flex_basis(&CssPropertyValue::Exact(LayoutFlexBasis::Exact(
                PixelValue::px(f32::INFINITY)
            ))),
            U32_SENTINEL
        );
    }

    // -------------------------------------------------------------------------
    // update_dom_declared_flags
    // -------------------------------------------------------------------------

    fn text_indent_prop() -> CssProperty {
        CssProperty::TextIndent(CssPropertyValue::Exact(StyleTextIndent::default()))
    }

    fn line_height_prop(pct: f32) -> CssProperty {
        CssProperty::LineHeight(CssPropertyValue::Exact(StyleLineHeight {
            inner: PercentageValue::new(pct),
        }))
    }

    #[test]
    fn dom_flags_set_the_right_bit_from_zero() {
        let mut flags = 0u32;
        update_dom_declared_flags(&text_indent_prop(), &mut flags);
        assert_eq!(flags, DOM_HAS_TEXT_INDENT);

        let mut flags2 = 0u32;
        update_dom_declared_flags(&line_height_prop(150.0), &mut flags2);
        assert_eq!(flags2, DOM_HAS_LINE_HEIGHT);
    }

    #[test]
    fn dom_flags_only_ever_or_never_clear() {
        // Starting from all-ones, the function must not clear a single bit.
        let mut flags = u32::MAX;
        update_dom_declared_flags(&text_indent_prop(), &mut flags);
        update_dom_declared_flags(&line_height_prop(150.0), &mut flags);
        update_dom_declared_flags(
            &CssProperty::Width(CssPropertyValue::Exact(LayoutWidth::px(10.0))),
            &mut flags,
        );
        assert_eq!(flags, u32::MAX);
    }

    #[test]
    fn dom_flags_accumulate_and_are_idempotent() {
        let mut flags = 0u32;
        update_dom_declared_flags(&text_indent_prop(), &mut flags);
        update_dom_declared_flags(&line_height_prop(150.0), &mut flags);
        let after_two = flags;
        assert_eq!(after_two, DOM_HAS_TEXT_INDENT | DOM_HAS_LINE_HEIGHT);
        // re-applying the same properties must be a no-op
        update_dom_declared_flags(&text_indent_prop(), &mut flags);
        update_dom_declared_flags(&line_height_prop(150.0), &mut flags);
        assert_eq!(flags, after_two);
    }

    #[test]
    fn dom_flags_are_not_set_for_a_valueless_property() {
        // `line-height: initial` / `text-indent: auto` carry no Exact payload, so the
        // "declared" fast-path bit must stay clear (the slow walk would find nothing).
        let mut flags = 0u32;
        update_dom_declared_flags(
            &CssProperty::LineHeight(CssPropertyValue::Initial),
            &mut flags,
        );
        update_dom_declared_flags(&CssProperty::TextIndent(CssPropertyValue::Auto), &mut flags);
        update_dom_declared_flags(
            &CssProperty::TextIndent(CssPropertyValue::Unset),
            &mut flags,
        );
        assert_eq!(flags, 0);
    }

    #[test]
    fn dom_flags_ignore_unrelated_properties() {
        let mut flags = 0u32;
        update_dom_declared_flags(
            &CssProperty::Width(CssPropertyValue::Exact(LayoutWidth::px(10.0))),
            &mut flags,
        );
        update_dom_declared_flags(
            &CssProperty::ZIndex(CssPropertyValue::Exact(LayoutZIndex::Integer(3))),
            &mut flags,
        );
        assert_eq!(flags, 0);
    }

    // -------------------------------------------------------------------------
    // apply_css_property_to_compact — tier 1 bitfield
    // -------------------------------------------------------------------------

    #[test]
    fn apply_tier1_fields_do_not_bleed_into_each_other() {
        let mut s = Sink::new();
        s.apply(&CssProperty::Display(CssPropertyValue::Exact(
            LayoutDisplay::InlineBlock,
        )));
        s.apply(&CssProperty::Position(CssPropertyValue::Exact(
            LayoutPosition::Absolute,
        )));
        // border-collapse lives at bit 52, i.e. at the far end of the bitfield
        s.apply(&CssProperty::BorderCollapse(CssPropertyValue::Exact(
            StyleBorderCollapse::Collapse,
        )));

        assert_eq!(
            (s.tier1 >> DISPLAY_SHIFT) & DISPLAY_MASK,
            u64::from(layout_display_to_u8(LayoutDisplay::InlineBlock))
        );
        assert_eq!(
            (s.tier1 >> POSITION_SHIFT) & POSITION_MASK,
            u64::from(layout_position_to_u8(LayoutPosition::Absolute))
        );
        assert_eq!(
            (s.tier1 >> BORDER_COLLAPSE_SHIFT) & BORDER_COLLAPSE_MASK,
            u64::from(border_collapse_to_u8(StyleBorderCollapse::Collapse))
        );

        let known = (DISPLAY_MASK << DISPLAY_SHIFT)
            | (POSITION_MASK << POSITION_SHIFT)
            | (BORDER_COLLAPSE_MASK << BORDER_COLLAPSE_SHIFT);
        assert_eq!(
            s.tier1 & !known,
            0,
            "tier1 = {:#x} has bits set outside the three fields that were written",
            s.tier1
        );
    }

    #[test]
    fn apply_tier1_overwrite_clears_only_its_own_field() {
        // Hostile starting state: every bit set. The clear-then-set in `set_tier1!`
        // must wipe exactly the display field and leave every neighbour intact.
        let mut s = Sink::new();
        s.tier1 = u64::MAX;
        s.apply(&CssProperty::Display(CssPropertyValue::Exact(
            LayoutDisplay::Block,
        )));
        assert_eq!(
            (s.tier1 >> DISPLAY_SHIFT) & DISPLAY_MASK,
            u64::from(layout_display_to_u8(LayoutDisplay::Block))
        );
        let others = !(DISPLAY_MASK << DISPLAY_SHIFT);
        assert_eq!(
            s.tier1 & others,
            u64::MAX & others,
            "neighbouring tier-1 fields were clobbered"
        );
    }

    #[test]
    fn apply_tier1_ignores_a_valueless_property() {
        let mut s = Sink::new();
        s.apply(&CssProperty::Display(CssPropertyValue::Inherit));
        assert_eq!(
            s.tier1, 0,
            "`display: inherit` has no Exact payload to encode"
        );
    }

    // -------------------------------------------------------------------------
    // apply_css_property_to_compact — tier 2 dims
    // -------------------------------------------------------------------------

    #[test]
    fn apply_width_round_trips_and_touches_nothing_else() {
        let mut s = Sink::new();
        let before_cold = s.cold;
        let before_text = s.text;
        s.apply(&CssProperty::Width(CssPropertyValue::Exact(
            LayoutWidth::Px(PixelValue::px(320.0)),
        )));
        let dec = decode_pixel_value_u32(s.dims.width).expect("width must round-trip");
        assert!((dec.number.get() - 320.0).abs() < 0.002);
        assert_eq!(
            s.tier1, 0,
            "a tier-2 property must not touch the tier-1 bitfield"
        );
        assert_eq!(
            s.cold, before_cold,
            "a tier-2 property must not touch tier-2 cold"
        );
        assert_eq!(
            s.text, before_text,
            "a tier-2 property must not touch tier-2b text"
        );
        assert!(s.fonts.is_empty());
    }

    #[test]
    fn apply_flex_grow_saturates_and_rejects_negatives() {
        let mut s = Sink::new();
        s.apply(&CssProperty::FlexGrow(CssPropertyValue::Exact(
            LayoutFlexGrow {
                inner: FloatValue::new(2.5),
            },
        )));
        assert_eq!(s.dims.flex_grow, 250);

        // A negative flex-grow must not wrap around into a huge positive u16.
        let mut neg = Sink::new();
        neg.apply(&CssProperty::FlexGrow(CssPropertyValue::Exact(
            LayoutFlexGrow {
                inner: FloatValue::new(-1.0),
            },
        )));
        assert_eq!(neg.dims.flex_grow, U16_SENTINEL);

        // ...and neither must an absurdly large one.
        let mut big = Sink::new();
        big.apply(&CssProperty::FlexGrow(CssPropertyValue::Exact(
            LayoutFlexGrow {
                inner: FloatValue::new(1.0e9),
            },
        )));
        assert_eq!(big.dims.flex_grow, U16_SENTINEL);

        // NaN degrades to 0 rather than to a wrapped value.
        let mut nan = Sink::new();
        nan.apply(&CssProperty::FlexShrink(CssPropertyValue::Exact(
            LayoutFlexShrink {
                inner: FloatValue::new(f32::NAN),
            },
        )));
        assert_eq!(nan.dims.flex_shrink, 0);
    }

    #[test]
    fn apply_gap_px_sets_both_axes_and_ignores_unresolvable_units() {
        let mut s = Sink::new();
        s.apply(&CssProperty::Gap(CssPropertyValue::Exact(LayoutGap {
            inner: PixelValue::px(8.0),
        })));
        assert_eq!(s.dims.row_gap, 80);
        assert_eq!(s.dims.column_gap, 80);

        // An `em` gap cannot be resolved without a font context — it must be left
        // untouched (so the slow path can handle it), not silently encoded as 2px.
        let mut em = Sink::new();
        em.apply(&CssProperty::Gap(CssPropertyValue::Exact(LayoutGap {
            inner: PixelValue::em(2.0),
        })));
        assert_eq!(em.dims.row_gap, 0);
        assert_eq!(em.dims.column_gap, 0);
    }

    // -------------------------------------------------------------------------
    // apply_css_property_to_compact — tier 2 cold
    // -------------------------------------------------------------------------

    #[test]
    fn apply_z_index_auto_and_in_range_values() {
        let mut s = Sink::new();
        s.apply(&CssProperty::ZIndex(CssPropertyValue::Exact(
            LayoutZIndex::Auto,
        )));
        assert_eq!(s.cold.z_index, I16_AUTO);
        s.apply(&CssProperty::ZIndex(CssPropertyValue::Exact(
            LayoutZIndex::Integer(100),
        )));
        assert_eq!(s.cold.z_index, 100);
        // last value below the sentinel band
        s.apply(&CssProperty::ZIndex(CssPropertyValue::Exact(
            LayoutZIndex::Integer(32_763),
        )));
        assert_eq!(s.cold.z_index, 32_763);
    }

    #[test]
    fn apply_z_index_large_positive_saturates() {
        for z in [32_764i32, 100_000, i32::MAX] {
            let mut s = Sink::new();
            s.apply(&CssProperty::ZIndex(CssPropertyValue::Exact(
                LayoutZIndex::Integer(z),
            )));
            assert_eq!(s.cold.z_index, I16_SENTINEL, "z-index {z} should saturate");
        }
    }

    #[test]
    fn apply_z_index_large_negative_must_not_wrap_positive() {
        // The encoder range-checks only the UPPER bound:
        //     if *z >= I16_SENTINEL_THRESHOLD { I16_SENTINEL } else { *z as i16 }
        // so a large negative z-index truncates instead of saturating, e.g.
        //     z-index: -40000  ->  -40000 as i16  ==  +25536
        // which flips the node from the very back of the stacking context to the
        // front. Compare with the line-height encoder, which *does* check
        // `pct_x10 >= -32768` before narrowing.
        for z in [-32_769i32, -40_000, -99_999, i32::MIN] {
            let mut s = Sink::new();
            s.apply(&CssProperty::ZIndex(CssPropertyValue::Exact(
                LayoutZIndex::Integer(z),
            )));
            assert!(
                s.cold.z_index < 0 || s.cold.z_index == I16_SENTINEL,
                "z-index {z} encoded to {}: a negative z-index must stay negative (or \
                 saturate to the sentinel), it must never wrap to a positive value",
                s.cold.z_index,
            );
        }
    }

    #[test]
    fn apply_border_styles_pack_into_independent_nibbles() {
        let mut s = Sink::new();
        s.apply(&CssProperty::BorderTopStyle(CssPropertyValue::Exact(
            StyleBorderTopStyle {
                inner: BorderStyle::Solid,
            },
        )));
        assert_eq!(
            s.cold.border_styles_packed & 0x000F,
            u16::from(border_style_to_u8(BorderStyle::Solid))
        );
        assert_eq!(
            s.cold.border_styles_packed & 0xFFF0,
            0,
            "the top-style nibble leaked into the other three sides"
        );

        // Re-applying must REPLACE the nibble, not OR into it: Solid(1) | Double(2)
        // would be Dotted(3), a different border style entirely.
        s.apply(&CssProperty::BorderTopStyle(CssPropertyValue::Exact(
            StyleBorderTopStyle {
                inner: BorderStyle::Double,
            },
        )));
        assert_eq!(
            s.cold.border_styles_packed & 0x000F,
            u16::from(border_style_to_u8(BorderStyle::Double))
        );
    }

    #[test]
    fn apply_opacity_clamps_into_the_0_254_range() {
        for (pct, expected) in [
            (-1.0e9f32, 0u8),
            (-100.0, 0),
            (0.0, 0),
            (50.0, 127),
            (100.0, 254),
            (500.0, 254),
            (1.0e9, 254),
        ] {
            let mut s = Sink::new();
            s.apply(&CssProperty::Opacity(CssPropertyValue::Exact(
                StyleOpacity {
                    inner: PercentageValue::new(pct),
                },
            )));
            assert_eq!(s.cold.opacity, expected, "opacity: {pct}%");
            assert_ne!(
                s.cold.opacity, OPACITY_SENTINEL,
                "an explicitly set opacity must never encode as the 'unset' sentinel"
            );
        }
    }

    #[test]
    fn apply_grid_column_encodes_both_lines() {
        let mut s = Sink::new();
        s.apply(&CssProperty::GridColumn(CssPropertyValue::Exact(
            GridPlacement {
                grid_start: GridLine::Line(2),
                grid_end: GridLine::Span(3),
            },
        )));
        assert_eq!(s.cold.grid_col_start, 2);
        assert_eq!(s.cold.grid_col_end, -3);
        // grid-row must be untouched by a grid-column declaration
        assert_eq!(s.cold.grid_row_start, I16_AUTO);
        assert_eq!(s.cold.grid_row_end, I16_AUTO);
    }

    #[test]
    fn apply_hot_flags_or_in_without_clobbering_each_other() {
        let mut s = Sink::new();
        s.apply(&CssProperty::TextDecoration(CssPropertyValue::Exact(
            StyleTextDecoration::Underline,
        )));
        assert_eq!(
            s.cold.hot_flags & HOT_FLAG_HAS_TEXT_DECORATION,
            HOT_FLAG_HAS_TEXT_DECORATION
        );

        // scrollbar-gutter writes a 2-bit *field* into the same byte; it must not
        // wipe the has-* bits around it.
        s.apply(&CssProperty::ScrollbarGutter(CssPropertyValue::Exact(
            StyleScrollbarGutter::Stable,
        )));
        assert_eq!(
            (s.cold.hot_flags & HOT_FLAG_SCROLLBAR_GUTTER_MASK) >> HOT_FLAG_SCROLLBAR_GUTTER_SHIFT,
            SCROLLBAR_GUTTER_STABLE
        );
        assert_eq!(
            s.cold.hot_flags & HOT_FLAG_HAS_TEXT_DECORATION,
            HOT_FLAG_HAS_TEXT_DECORATION,
            "scrollbar-gutter cleared the has-text-decoration bit"
        );

        // ...and replacing the gutter value must clear the old bits, not OR into them
        s.apply(&CssProperty::ScrollbarGutter(CssPropertyValue::Exact(
            StyleScrollbarGutter::Auto,
        )));
        assert_eq!(
            (s.cold.hot_flags & HOT_FLAG_SCROLLBAR_GUTTER_MASK) >> HOT_FLAG_SCROLLBAR_GUTTER_SHIFT,
            SCROLLBAR_GUTTER_AUTO
        );
        assert_eq!(
            s.cold.hot_flags & HOT_FLAG_HAS_TEXT_DECORATION,
            HOT_FLAG_HAS_TEXT_DECORATION
        );
    }

    #[test]
    fn apply_valueless_property_does_not_set_a_has_flag() {
        // The has-* bits exist so the getter can skip the cascade walk. A property
        // with no Exact payload must leave them clear, or every node pays for a walk
        // that would find nothing.
        let mut s = Sink::new();
        s.apply(&CssProperty::TextDecoration(CssPropertyValue::Initial));
        s.apply(&CssProperty::ScrollbarGutter(CssPropertyValue::Unset));
        assert_eq!(s.cold.hot_flags, 0);
    }

    // -------------------------------------------------------------------------
    // apply_css_property_to_compact — tier 2b text
    // -------------------------------------------------------------------------

    #[test]
    fn apply_text_color_packs_rgba_big_endian() {
        let mut s = Sink::new();
        s.apply(&CssProperty::TextColor(CssPropertyValue::Exact(
            StyleTextColor {
                inner: ColorU {
                    r: 0x12,
                    g: 0x34,
                    b: 0x56,
                    a: 0x78,
                },
            },
        )));
        assert_eq!(s.text.text_color, 0x1234_5678);

        // Documented limitation: rgba(0,0,0,0) is indistinguishable from "unset".
        let mut transparent = Sink::new();
        transparent.apply(&CssProperty::TextColor(CssPropertyValue::Exact(
            StyleTextColor {
                inner: ColorU {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 0,
                },
            },
        )));
        assert_eq!(transparent.text.text_color, 0);
    }

    #[test]
    fn apply_line_height_round_trips_and_saturates_at_both_ends() {
        let mut s = Sink::new();
        s.apply(&line_height_prop(120.0));
        assert_eq!(s.text.line_height, 1200, "120% must encode as % x 10");

        // Absurd values must saturate - no wrap-around. The two signs land
        // differently by design: a huge POSITIVE (unitless multiple) falls to
        // the sentinel ("normal" - a 10^7x multiple is meaningless), while a
        // huge NEGATIVE (= absolute px per the parser convention) CLAMPS to
        // the largest representable px (-32768 = 3276.8px) instead of being
        // silently reinterpreted as "normal".
        let mut big = Sink::new();
        big.apply(&line_height_prop(1.0e9f32));
        assert_eq!(
            big.text.line_height, I16_SENTINEL,
            "a huge unitless multiple saturates to the sentinel"
        );
        let mut neg = Sink::new();
        neg.apply(&line_height_prop(-1.0e9f32));
        assert_eq!(
            neg.text.line_height, -32768,
            "a huge absolute px line-height clamps instead of dropping to normal"
        );

        // The split scale itself: 48px (normalized -48) stores as -480 and
        // decodes back to 48px - the old x1000 scale overflowed at 32.76px.
        let mut px48 = Sink::new();
        px48.apply(&line_height_prop(-4800.0));
        assert_eq!(
            px48.text.line_height, -480,
            "line-height: 48px stores as -px x 10"
        );
    }

    #[test]
    fn apply_font_family_hash_is_nonzero_stable_and_registered() {
        let arial = StyleFontFamilyVec::from_vec(vec![StyleFontFamily::System("Arial".into())]);

        let mut s = Sink::new();
        s.apply(&CssProperty::FontFamily(CssPropertyValue::Exact(
            arial.clone(),
        )));
        let h = s.text.font_family_hash;
        assert_ne!(
            h, 0,
            "0 is the 'unset' sentinel — a set font-family must never hash to it"
        );
        assert!(
            s.fonts.contains_key(&h),
            "the hash must be registered in the reverse map, or consumers cannot resolve it"
        );

        // Same input -> same hash (the whole dirty-tracking scheme depends on this).
        let mut same = Sink::new();
        same.apply(&CssProperty::FontFamily(CssPropertyValue::Exact(arial)));
        assert_eq!(same.text.font_family_hash, h);

        // Different input -> different hash.
        let mut other = Sink::new();
        other.apply(&CssProperty::FontFamily(CssPropertyValue::Exact(
            StyleFontFamilyVec::from_vec(vec![StyleFontFamily::System("Times".into())]),
        )));
        assert_ne!(other.text.font_family_hash, h);
    }

    // -------------------------------------------------------------------------
    // apply_ua_css_to_compact
    // -------------------------------------------------------------------------

    #[test]
    fn ua_css_is_idempotent_for_every_representative_node_type() {
        let nodes = [
            NodeData::create_node(NodeType::Html),
            NodeData::create_node(NodeType::Body),
            NodeData::create_node(NodeType::Div),
            NodeData::create_node(NodeType::P),
            NodeData::create_node(NodeType::Br),
            NodeData::create_text_do_not_use_without_block_level_wrapper("hello"),
        ];
        for nd in &nodes {
            let mut s = Sink::new();
            s.ua(&nd.node_type);
            let once = s.snapshot();
            s.ua(&nd.node_type);
            assert_eq!(
                s.snapshot(),
                once,
                "applying UA CSS twice must be a no-op the second time"
            );
        }
    }

    #[test]
    fn ua_css_never_touches_the_tier1_populated_bit() {
        // Bit 63 is owned by the builder, not by the UA stylesheet.
        for nt in [NodeType::Html, NodeType::Body, NodeType::Div, NodeType::P] {
            let mut s = Sink::new();
            s.ua(&nt);
            assert_eq!(s.tier1 & TIER1_POPULATED_BIT, 0);
        }
    }

    #[test]
    fn ua_css_survives_a_hostile_pre_filled_sink() {
        // Every bit set / every numeric field at an extreme: the writer must still
        // only touch its own fields and must not panic on the sentinel inputs.
        let mut s = Sink::new();
        s.tier1 = u64::MAX;
        s.dims.width = U32_SENTINEL;
        s.dims.font_size = U32_SENTINEL;
        s.dims.flex_grow = U16_SENTINEL;
        s.cold.z_index = i16::MIN;
        s.cold.opacity = OPACITY_SENTINEL;
        s.text.line_height = i16::MIN;
        s.ua(&NodeType::Div);
        assert_eq!(
            s.tier1 & TIER1_POPULATED_BIT,
            TIER1_POPULATED_BIT,
            "UA CSS must not clear bits it does not own"
        );
    }

    // -------------------------------------------------------------------------
    // build_compact_cache
    // -------------------------------------------------------------------------

    #[test]
    fn build_compact_cache_handles_zero_nodes() {
        let cache = CssPropertyCache::empty(0);
        let r = cache.build_compact_cache(&[], &[]);
        assert_eq!(r.node_count(), 0);
        assert!(r.tier2_dims.is_empty());
        assert!(r.font_dirty_nodes.is_empty());
        assert!(r.prev_font_hashes.is_empty());
    }

    #[test]
    fn build_compact_cache_tolerates_a_mismatched_prev_font_hash_slice() {
        let cache = CssPropertyCache::empty(3);
        let nodes = div_nodes(3);
        // longer than node_count, shorter than node_count, and empty — none may panic
        for prev in [vec![1u64, 2, 3, 4, 5, 6], vec![7u64], Vec::new()] {
            let r = cache.build_compact_cache(&nodes, &prev);
            assert_eq!(r.prev_font_hashes.len(), 3);
            assert_eq!(r.node_count(), 3);
        }
    }

    #[test]
    fn build_compact_cache_tolerates_short_node_data() {
        // node_count claims 4 but only 2 NodeDatas are supplied: the trailing nodes
        // must keep their defaults instead of indexing out of bounds.
        let cache = CssPropertyCache::empty(4);
        let r = cache.build_compact_cache(&div_nodes(2), &[]);
        assert_eq!(r.node_count(), 4);
        assert_eq!(r.tier2_dims.len(), 4);
        assert_eq!(r.tier2_cold.len(), 4);
        assert_eq!(r.tier2b_text.len(), 4);
        assert_eq!(r.prev_font_hashes.len(), 4);
    }

    #[test]
    fn build_compact_cache_honours_node_count_over_node_data_len() {
        let cache = CssPropertyCache::empty(2);
        let r = cache.build_compact_cache(&div_nodes(5), &[]);
        assert_eq!(r.node_count(), 2);
    }

    #[test]
    fn build_compact_cache_rebuild_with_unchanged_fonts_is_not_dirty() {
        let cache = CssPropertyCache::empty(3);
        let nodes = div_nodes(3);
        let first = cache.build_compact_cache(&nodes, &[]);
        let second = cache.build_compact_cache(&nodes, &first.prev_font_hashes);
        assert!(
            second.font_dirty_nodes.is_empty(),
            "a rebuild with identical font hashes must not re-resolve any font chain"
        );
    }

    // -------------------------------------------------------------------------
    // build_compact_cache_with_inheritance{,_debug}
    // -------------------------------------------------------------------------

    #[test]
    fn build_with_inheritance_handles_zero_nodes() {
        let cache = CssPropertyCache::empty(0);
        let r = cache.build_compact_cache_with_inheritance(&[], &[], &[]);
        assert_eq!(r.node_count(), 0);

        let mut msgs = None;
        let r2 = cache.build_compact_cache_with_inheritance_debug(&[], &[], &[], &mut msgs);
        assert_eq!(r2.node_count(), 0);
        assert!(msgs.is_none());
    }

    #[test]
    fn build_with_inheritance_propagates_font_size_down_the_chain() {
        let n = 3;
        let cache = CssPropertyCache::empty(n);
        let r =
            cache.build_compact_cache_with_inheritance(&div_nodes(n), &linear_hierarchy(n), &[]);
        assert_eq!(r.node_count(), n);
        // font-size is inheritable: property-less children must match the root exactly.
        assert_eq!(r.tier2_dims[1].font_size, r.tier2_dims[0].font_size);
        assert_eq!(r.tier2_dims[2].font_size, r.tier2_dims[0].font_size);
    }

    #[test]
    fn build_with_inheritance_marks_all_nodes_dirty_on_the_first_build() {
        let n = 3;
        let cache = CssPropertyCache::empty(n);
        let nodes = div_nodes(n);
        let hierarchy = linear_hierarchy(n);

        // Empty prev_font_hashes == first build for this DOM -> force ALL nodes dirty.
        let first = cache.build_compact_cache_with_inheritance(&nodes, &hierarchy, &[]);
        assert_eq!(first.font_dirty_nodes, vec![0, 1, 2]);

        // Second build with the previous hashes -> nothing changed, nothing dirty.
        let second =
            cache.build_compact_cache_with_inheritance(&nodes, &hierarchy, &first.prev_font_hashes);
        assert!(second.font_dirty_nodes.is_empty());
    }

    #[test]
    fn build_with_inheritance_global_star_rules_skip_text_nodes() {
        // Per CSS, `*` matches ELEMENTS. A text node is not an element — it may only
        // inherit from its parent, otherwise `* { padding: 5px }` would overwrite the
        // value a text node inherited from `<p>`.
        let mut cache = CssPropertyCache::empty(2);
        cache
            .global_css_props
            .push(CssProperty::PaddingTop(padding(5.0)));

        let nodes = vec![
            NodeData::create_node(NodeType::Div),
            NodeData::create_text_do_not_use_without_block_level_wrapper("hi"),
        ];
        let r = cache.build_compact_cache_with_inheritance(&nodes, &linear_hierarchy(2), &[]);

        assert_eq!(
            r.tier2_dims[0].padding_top, 50,
            "the `*` rule must apply to the element"
        );
        assert_ne!(
            r.tier2_dims[1].padding_top, 50,
            "the `*` rule must NOT apply to a text node"
        );
    }

    #[test]
    fn build_with_inheritance_debug_messages_are_opt_in() {
        let n = 2;
        let cache = CssPropertyCache::empty(n);
        let nodes = div_nodes(n);
        let hierarchy = linear_hierarchy(n);

        let mut on = Some(Vec::new());
        let _ = cache.build_compact_cache_with_inheritance_debug(&nodes, &hierarchy, &[], &mut on);
        assert!(
            !on.expect("still Some").is_empty(),
            "debug logging must emit at least one cascade message"
        );

        let mut off = None;
        let _ = cache.build_compact_cache_with_inheritance_debug(&nodes, &hierarchy, &[], &mut off);
        assert!(off.is_none(), "a None sink must stay None");
    }

    // -------------------------------------------------------------------------
    // resolve_font_size_to_px
    // -------------------------------------------------------------------------

    #[test]
    fn resolve_font_size_percent_uses_the_parent() {
        let mut dims = vec![CompactNodeProps::default(); 2];
        dims[0].font_size = encode_pixel_value_u32(&PixelValue::px(20.0));
        dims[1].font_size = encode_pixel_value_u32(&PixelValue::percent(50.0));
        resolve_font_size_to_px(&mut dims, 1, Some(NodeId::new(0)));
        let pv = decode_pixel_value_u32(dims[1].font_size).expect("must resolve to px");
        assert_eq!(pv.metric, SizeMetric::Px);
        assert!(
            (pv.number.get() - 10.0).abs() < 0.01,
            "got {}",
            pv.number.get()
        );
    }

    #[test]
    fn resolve_font_size_pt_converts_to_px() {
        let mut dims = vec![CompactNodeProps::default()];
        dims[0].font_size = encode_pixel_value_u32(&PixelValue::pt(12.0));
        resolve_font_size_to_px(&mut dims, 0, None);
        let pv = decode_pixel_value_u32(dims[0].font_size).expect("must resolve to px");
        assert!(
            (pv.number.get() - 16.0).abs() < 0.01,
            "12pt should be 16px, got {}",
            pv.number.get()
        );
    }

    #[test]
    fn resolve_font_size_leaves_absolute_and_sentinel_values_alone() {
        // an already-px value must not be re-scaled by the parent
        let mut dims = vec![CompactNodeProps::default(); 2];
        dims[0].font_size = encode_pixel_value_u32(&PixelValue::px(20.0));
        dims[1].font_size = encode_pixel_value_u32(&PixelValue::px(13.0));
        let before = dims[1].font_size;
        resolve_font_size_to_px(&mut dims, 1, Some(NodeId::new(0)));
        assert_eq!(dims[1].font_size, before);

        // an explicit sentinel must survive untouched
        let mut sent = vec![CompactNodeProps::default(); 2];
        sent[0].font_size = encode_pixel_value_u32(&PixelValue::px(20.0));
        sent[1].font_size = U32_SENTINEL;
        resolve_font_size_to_px(&mut sent, 1, Some(NodeId::new(0)));
        assert_eq!(sent[1].font_size, U32_SENTINEL);

        // ...as must the CSS-initial default (which also sits above the threshold)
        let mut def = vec![CompactNodeProps::default(); 2];
        assert_eq!(def[1].font_size, U32_INITIAL);
        resolve_font_size_to_px(&mut def, 1, Some(NodeId::new(0)));
        assert_eq!(def[1].font_size, U32_INITIAL);
    }

    #[test]
    fn resolve_font_size_negative_em_is_deterministic() {
        let mut dims = vec![CompactNodeProps::default(); 2];
        dims[0].font_size = encode_pixel_value_u32(&PixelValue::px(20.0));
        dims[1].font_size = encode_pixel_value_u32(&PixelValue::em(-2.0));
        resolve_font_size_to_px(&mut dims, 1, Some(NodeId::new(0)));
        let pv = decode_pixel_value_u32(dims[1].font_size).expect("must stay decodable");
        assert!(
            (pv.number.get() + 40.0).abs() < 0.01,
            "-2em of 20px should be -40px, got {}",
            pv.number.get()
        );
    }

    #[test]
    fn resolve_font_size_overflow_saturates_instead_of_wrapping() {
        let mut dims = vec![CompactNodeProps::default(); 2];
        dims[0].font_size = encode_pixel_value_u32(&PixelValue::px(20.0));
        // 100_000em x 20px = 2_000_000px, past the 28-bit fixed-point range
        dims[1].font_size = encode_pixel_value_u32(&PixelValue::em(100_000.0));
        resolve_font_size_to_px(&mut dims, 1, Some(NodeId::new(0)));
        assert_eq!(
            dims[1].font_size, U32_SENTINEL,
            "an overflowing font-size must land on the tier-3 sentinel, not wrap"
        );
    }

    #[test]
    fn resolve_font_size_nan_em_degrades_to_zero() {
        let mut dims = vec![CompactNodeProps::default(); 2];
        dims[0].font_size = encode_pixel_value_u32(&PixelValue::px(20.0));
        dims[1].font_size = encode_pixel_value_u32(&PixelValue::em(f32::NAN));
        resolve_font_size_to_px(&mut dims, 1, Some(NodeId::new(0)));
        let pv = decode_pixel_value_u32(dims[1].font_size).expect("must stay decodable");
        assert!(
            pv.number.get().is_finite(),
            "a NaN font-size must not propagate"
        );
        assert_eq!(pv.number.get(), 0.0);
    }

    #[test]
    fn resolve_font_size_root_rem_uses_the_16px_initial_value() {
        // For the ROOT node, `tier2_dims.first()` IS the node itself — and at this
        // point its font-size is still the *unresolved* rem value. The Rem arm then
        // multiplies the rem factor by itself:
        //     html { font-size: 2rem }  ->  2 * 2 = 4px   (should be 2 * 16 = 32px)
        // Every other unit handles the no-parent case correctly via `map_or(16.0, ..)`.
        let mut dims = vec![CompactNodeProps::default()];
        dims[0].font_size = encode_pixel_value_u32(&PixelValue::rem(2.0));
        resolve_font_size_to_px(&mut dims, 0, None);
        let pv = decode_pixel_value_u32(dims[0].font_size).expect("must resolve to px");
        assert!(
            (pv.number.get() - 32.0).abs() < 0.01,
            "root `font-size: 2rem` should resolve against the 16px initial value (= 32px), \
             got {}px",
            pv.number.get()
        );
    }
}
