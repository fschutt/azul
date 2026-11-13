    use azul_core::dom::{AttributeType, Dom, IdOrClass};

    use super::*;
    use crate::{
        solver3::geometry::{MarginAuto, PackedBoxProps},
        text3::cache::{OverflowInfo, TextTransform, UnifiedLayout},
    };

    // ------------------------------------------------------------------
    // Fixtures
    // ------------------------------------------------------------------

    const HTB: LayoutWritingMode = LayoutWritingMode::HorizontalTb;
    const VRL: LayoutWritingMode = LayoutWritingMode::VerticalRl;

    fn size(w: f32, h: f32) -> LogicalSize {
        LogicalSize::new(w, h)
    }

    fn edges(top: f32, right: f32, bottom: f32, left: f32) -> EdgeSizes {
        EdgeSizes {
            top,
            right,
            bottom,
            left,
        }
    }

    fn rect(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
        LogicalRect::new(LogicalPosition::new(x, y), size(w, h))
    }

    fn box_props(margin: EdgeSizes, border: EdgeSizes, padding: EdgeSizes) -> BoxProps {
        BoxProps {
            margin,
            padding,
            border,
            margin_auto: MarginAuto::default(),
        }
    }

    fn hot(parent: Option<usize>, used_size: Option<LogicalSize>, bp: &BoxProps) -> LayoutNodeHot {
        LayoutNodeHot {
            box_props: PackedBoxProps::pack(bp),
            dom_node_id: None,
            used_size,
            formatting_context: FormattingContext::Block {
                establishes_new_context: false,
            },
            parent,
        }
    }

    /// Builds a `LayoutTree` from hot nodes + per-node child lists.
    fn build_tree(
        nodes: Vec<LayoutNodeHot>,
        warm: Vec<LayoutNodeWarm>,
        child_lists: &[Vec<usize>],
    ) -> LayoutTree {
        let n = nodes.len();
        let mut children_arena: Vec<usize> = Vec::new();
        let mut children_offsets: Vec<(u32, u32)> = Vec::with_capacity(n);
        for cl in child_lists {
            let start = u32::try_from(children_arena.len()).unwrap();
            children_arena.extend_from_slice(cl);
            children_offsets.push((start, u32::try_from(cl.len()).unwrap()));
        }
        while children_offsets.len() < n {
            children_offsets.push((0, 0));
        }
        LayoutTree {
            nodes,
            warm,
            cold: vec![LayoutNodeCold::default(); n],
            root: 0,
            dom_to_layout: BTreeMap::new(),
            children_arena,
            children_offsets,
            subtree_needs_intrinsic: Vec::new(),
        }
    }

    /// An inline layout result carrying `item_count` (always-empty) items.
    /// Only `items.is_empty()` is ever inspected by the functions under test.
    fn empty_inline_layout() -> CachedInlineLayout {
        CachedInlineLayout::new(
            Arc::new(UnifiedLayout {
                items: Vec::new(),
                overflow: OverflowInfo::default(),
            }),
            Text3AvailableSpace::MaxContent,
            false,
        )
    }

    fn constraints(available: LogicalSize, wm: LayoutWritingMode) -> LayoutConstraints<'static> {
        LayoutConstraints {
            available_size: available,
            writing_mode: wm,
            writing_mode_ctx: super::super::geometry::WritingModeContext::default(),
            bfc_state: None,
            text_align: TextAlign::Start,
            containing_block_size: available,
            available_width_type: Text3AvailableSpace::Definite(available.width),
            fragmentainer: None,
            column_flow: None,
        }
    }

    fn styled(dom: Dom, css_str: &str) -> StyledDom {
        let mut dom = dom;
        let (css, _warnings) = azul_css::parser2::new_from_str(css_str);
        StyledDom::create(&mut dom, css)
    }

    /// `body(0) > div.p(1) > text(2)` — DOM ids are the depth-first pre-order index.
    fn text_dom(text: &str, css_str: &str) -> StyledDom {
        styled(
            Dom::create_body().with_child(
                Dom::create_div()
                    .with_ids_and_classes(vec![IdOrClass::Class("p".into())].into())
                    .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                        text,
                    )),
            ),
            css_str,
        )
    }

    const TEXT_NODE: NodeId = NodeId::new(2);
    const DIV_NODE: NodeId = NodeId::new(1);

    fn plain_style() -> Arc<StyleProperties> {
        Arc::new(StyleProperties::default())
    }

    fn text_of(item: &InlineContent) -> Option<&str> {
        match item {
            InlineContent::Text(run) => Some(&*run.text),
            _ => None,
        }
    }

    // ==================================================================
    // OverflowBehavior (predicates)
    // ==================================================================

    const ALL_OVERFLOW: [OverflowBehavior; 5] = [
        OverflowBehavior::Visible,
        OverflowBehavior::Hidden,
        OverflowBehavior::Clip,
        OverflowBehavior::Scroll,
        OverflowBehavior::Auto,
    ];

    #[test]
    fn overflow_behavior_is_clipped_is_exhaustive_and_total() {
        assert!(!OverflowBehavior::Visible.is_clipped());
        assert!(OverflowBehavior::Hidden.is_clipped());
        assert!(OverflowBehavior::Clip.is_clipped());
        assert!(OverflowBehavior::Scroll.is_clipped());
        assert!(OverflowBehavior::Auto.is_clipped());
        // Visible is the only non-clipping value.
        assert_eq!(ALL_OVERFLOW.iter().filter(|o| o.is_clipped()).count(), 4);
    }

    #[test]
    fn overflow_behavior_is_scroll_only_for_scroll_and_auto() {
        assert!(!OverflowBehavior::Visible.is_scroll());
        assert!(!OverflowBehavior::Hidden.is_scroll());
        assert!(!OverflowBehavior::Clip.is_scroll());
        assert!(OverflowBehavior::Scroll.is_scroll());
        assert!(OverflowBehavior::Auto.is_scroll());
    }

    #[test]
    fn overflow_behavior_scroll_implies_clipped_invariant() {
        // A scrollable box always clips: is_scroll() must be a subset of is_clipped().
        for o in ALL_OVERFLOW {
            assert!(
                !o.is_scroll() || o.is_clipped(),
                "{o:?} is scroll but not clipped"
            );
        }
    }

    // ==================================================================
    // BfcLayoutResult::from_output / BfcState::new / TableLayoutContext::new
    // ==================================================================

    #[test]
    fn bfc_layout_result_from_output_preserves_output_and_nulls_escaped_margins() {
        let mut positions = BTreeMap::new();
        positions.insert(usize::MAX, LogicalPosition::new(f32::MIN, f32::MAX));
        let output = LayoutOutput {
            positions,
            overflow_size: size(f32::NAN, f32::INFINITY),
            baseline: Some(-0.0),
            static_positions: BTreeMap::new(),
        };
        let res = BfcLayoutResult::from_output(output);
        assert!(res.escaped_top_margin.is_none());
        assert!(res.escaped_bottom_margin.is_none());
        assert_eq!(res.output.positions.len(), 1);
        assert!(res.output.overflow_size.width.is_nan());
        assert!(res.output.overflow_size.height.is_infinite());
        assert_eq!(res.output.baseline, Some(-0.0));
    }

    #[test]
    fn bfc_state_new_matches_default_and_starts_empty() {
        let s = BfcState::new();
        assert_eq!(s.pen, LogicalPosition::zero());
        assert!(s.floats.floats.is_empty());
        assert_eq!(s.margins.last_in_flow_margin_bottom, 0.0);

        let d = BfcState::default();
        assert_eq!(d.pen, s.pen);
        assert!(d.floats.floats.is_empty());
    }

    #[test]
    fn table_layout_context_new_starts_empty_and_separate() {
        let t = TableLayoutContext::new();
        assert!(t.columns.is_empty());
        assert!(t.cells.is_empty());
        assert_eq!(t.num_rows, 0);
        assert!(!t.use_fixed_layout);
        assert!(t.row_heights.is_empty());
        assert!(t.row_baselines.is_empty());
        assert!(matches!(t.border_collapse, StyleBorderCollapse::Separate));
        assert!(t.caption_index.is_none());
        assert!(t.collapsed_rows.is_empty());
        assert!(t.collapsed_columns.is_empty());
        assert!(t.hidden_empty_rows.is_empty());
        assert!(t.row_node_indices.is_empty());
    }

    // ==================================================================
    // FloatingContext (numeric)
    // ==================================================================

    #[test]
    fn add_float_accepts_extreme_geometry_without_panicking() {
        let mut fc = FloatingContext::default();
        fc.add_float(
            LayoutFloat::Left,
            rect(0.0, 0.0, 0.0, 0.0),
            EdgeSizes::default(),
        );
        fc.add_float(
            LayoutFloat::Right,
            rect(f32::MIN, f32::MIN, f32::MAX, f32::MAX),
            edges(f32::MAX, f32::MAX, f32::MAX, f32::MAX),
        );
        fc.add_float(
            LayoutFloat::None,
            rect(f32::NAN, f32::NAN, f32::NAN, f32::NAN),
            edges(f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.0),
        );
        assert_eq!(fc.floats.len(), 3);
        // The context is a pure accumulator: nothing is normalised or rejected.
        assert!(fc.floats[2].rect.size.width.is_nan());
    }

    #[test]
    fn available_line_box_space_with_no_floats_returns_the_full_band() {
        let fc = FloatingContext::default();
        assert_eq!(fc.available_line_box_space(0.0, 0.0, 0.0, HTB), (0.0, 0.0));
        assert_eq!(
            fc.available_line_box_space(0.0, 20.0, 300.0, HTB),
            (0.0, 300.0)
        );
        // A negative BFC size is passed straight through (start > end: an empty band).
        assert_eq!(
            fc.available_line_box_space(-50.0, -10.0, -5.0, HTB),
            (0.0, -5.0)
        );
        let (s, e) = fc.available_line_box_space(f32::MIN, f32::MAX, f32::MAX, HTB);
        assert_eq!(s, 0.0);
        assert_eq!(e, f32::MAX);
    }

    #[test]
    fn available_line_box_space_subtracts_left_and_right_float_margin_boxes() {
        let mut fc = FloatingContext::default();
        // Left float: content 0..100 on the cross axis, +10px margins on each side.
        fc.add_float(
            LayoutFloat::Left,
            rect(10.0, 10.0, 100.0, 50.0),
            edges(10.0, 10.0, 10.0, 10.0),
        );
        // Right float: content box ends at 290, +10px margin -> starts at 280.
        fc.add_float(
            LayoutFloat::Right,
            rect(200.0, 10.0, 90.0, 50.0),
            edges(10.0, 10.0, 10.0, 10.0),
        );

        // Line inside the floats' main-axis band: both margin boxes are excluded.
        let (start, end) = fc.available_line_box_space(10.0, 20.0, 300.0, HTB);
        assert_eq!(start, 120.0); // 10 (origin) - 10 (margin) + 100 + 10 + 10
        assert_eq!(end, 190.0); // 200 - 10 (left margin of the right float)

        // A line entirely below both floats sees the full band again.
        assert_eq!(
            fc.available_line_box_space(1000.0, 1010.0, 300.0, HTB),
            (0.0, 300.0)
        );
    }

    #[test]
    fn available_line_box_space_main_axis_overlap_is_half_open() {
        let mut fc = FloatingContext::default();
        // Margin box spans main 0..50 exactly (no margins).
        fc.add_float(
            LayoutFloat::Left,
            rect(0.0, 0.0, 100.0, 50.0),
            EdgeSizes::default(),
        );
        // A line starting exactly at the float's bottom edge does NOT overlap.
        assert_eq!(
            fc.available_line_box_space(50.0, 70.0, 300.0, HTB),
            (0.0, 300.0)
        );
        // A zero-height line just inside the float does not overlap either
        // (main_end > float_start is false at the very top edge).
        assert_eq!(
            fc.available_line_box_space(0.0, 0.0, 300.0, HTB),
            (0.0, 300.0)
        );
        // One pixel of overlap is enough.
        assert_eq!(
            fc.available_line_box_space(49.0, 70.0, 300.0, HTB),
            (100.0, 300.0)
        );
    }

    #[test]
    fn available_line_box_space_ignores_nan_geometry_instead_of_panicking() {
        let mut fc = FloatingContext::default();
        fc.add_float(
            LayoutFloat::Left,
            rect(f32::NAN, f32::NAN, f32::NAN, f32::NAN),
            EdgeSizes::default(),
        );
        // Every NaN comparison is false, so the float never registers as overlapping.
        assert_eq!(
            fc.available_line_box_space(0.0, 20.0, 300.0, HTB),
            (0.0, 300.0)
        );
        // A NaN query range is likewise inert.
        let (s, e) = fc.available_line_box_space(f32::NAN, f32::NAN, 300.0, HTB);
        assert_eq!((s, e), (0.0, 300.0));
    }

    #[test]
    fn available_line_box_space_is_writing_mode_aware() {
        let mut fc = FloatingContext::default();
        // vertical-rl: main = x, cross = y.
        fc.add_float(
            LayoutFloat::Left,
            rect(0.0, 0.0, 50.0, 100.0),
            EdgeSizes::default(),
        );
        // Same rect, read in vertical-rl: main span 0..50 (x), cross 0..100 (y).
        assert_eq!(
            fc.available_line_box_space(10.0, 20.0, 300.0, VRL),
            (100.0, 300.0)
        );
        // In horizontal-tb the very same float spans main 0..100 (y), cross 0..50 (x).
        assert_eq!(
            fc.available_line_box_space(10.0, 20.0, 300.0, HTB),
            (50.0, 300.0)
        );
    }

    #[test]
    fn clearance_offset_never_moves_content_upwards() {
        let mut fc = FloatingContext::default();
        fc.add_float(
            LayoutFloat::Left,
            rect(0.0, 0.0, 100.0, 80.0),
            edges(0.0, 0.0, 20.0, 0.0), // 20px bottom margin -> outer edge at 100
        );
        fc.add_float(
            LayoutFloat::Right,
            rect(200.0, 0.0, 100.0, 40.0),
            EdgeSizes::default(),
        );

        assert_eq!(fc.clearance_offset(LayoutClear::Left, 0.0, HTB), 100.0);
        assert_eq!(fc.clearance_offset(LayoutClear::Right, 0.0, HTB), 40.0);
        assert_eq!(fc.clearance_offset(LayoutClear::Both, 0.0, HTB), 100.0);
        // clear:none never consults the floats.
        assert_eq!(fc.clearance_offset(LayoutClear::None, 25.0, HTB), 25.0);
        // Already below every float: the pen stays where it is (no negative clearance).
        assert_eq!(fc.clearance_offset(LayoutClear::Both, 500.0, HTB), 500.0);
    }

    #[test]
    fn clearance_offset_clamps_negative_offsets_to_zero() {
        let fc = FloatingContext::default();
        // Documented consequence of `max_end_offset` starting at 0.0: with no floats
        // at all, a negative pen is still pulled up to 0 rather than passed through.
        assert_eq!(fc.clearance_offset(LayoutClear::Both, -10.0, HTB), 0.0);
        assert_eq!(fc.clearance_offset(LayoutClear::None, -10.0, HTB), 0.0);
        // Non-negative offsets are untouched.
        assert_eq!(fc.clearance_offset(LayoutClear::None, 0.0, HTB), 0.0);
    }

    #[test]
    fn clearance_offset_handles_nan_and_infinite_floats() {
        let mut fc = FloatingContext::default();
        fc.add_float(
            LayoutFloat::Left,
            rect(0.0, 0.0, 10.0, f32::INFINITY),
            EdgeSizes::default(),
        );
        assert!(fc
            .clearance_offset(LayoutClear::Left, 0.0, HTB)
            .is_infinite());

        // A NaN pen short-circuits the `>` test and is returned unchanged.
        assert!(fc
            .clearance_offset(LayoutClear::Left, f32::NAN, HTB)
            .is_nan());

        let mut nan_fc = FloatingContext::default();
        nan_fc.add_float(
            LayoutFloat::Left,
            rect(f32::NAN, f32::NAN, f32::NAN, f32::NAN),
            EdgeSizes::default(),
        );
        // f32::max() discards NaN, so a NaN float contributes nothing.
        assert_eq!(nan_fc.clearance_offset(LayoutClear::Left, 5.0, HTB), 5.0);
    }

    #[test]
    fn clearance_offset_saturates_at_float_extremes() {
        let mut fc = FloatingContext::default();
        fc.add_float(
            LayoutFloat::Right,
            rect(f32::MAX, 0.0, f32::MAX, f32::MAX),
            edges(0.0, 0.0, f32::MAX, 0.0),
        );
        let out = fc.clearance_offset(LayoutClear::Right, f32::MIN, HTB);
        // MAX + MAX + MAX saturates to +inf rather than wrapping or panicking.
        assert!(out.is_infinite() && out.is_sign_positive());
    }

    // ==================================================================
    // position_float (numeric)
    // ==================================================================

    #[test]
    fn position_float_places_left_and_right_floats_at_the_band_edges() {
        let fc = FloatingContext::default();
        let m = edges(10.0, 10.0, 10.0, 10.0);

        let left = position_float(
            &fc,
            LayoutFloat::Left,
            size(100.0, 50.0),
            &m,
            0.0,
            300.0,
            HTB,
        );
        assert_eq!(left.origin.x, 10.0); // cross-start + left margin
        assert_eq!(left.origin.y, 10.0); // main offset + top margin
        assert_eq!(left.size, size(100.0, 50.0)); // the border box is passed through

        let right = position_float(
            &fc,
            LayoutFloat::Right,
            size(100.0, 50.0),
            &m,
            0.0,
            300.0,
            HTB,
        );
        // 300 - (100 + 10 + 10) + 10 => content box ends at 290, margin box at 300.
        assert_eq!(right.origin.x, 190.0);
        assert_eq!(right.origin.y, 10.0);
    }

    #[test]
    fn position_float_at_zero_size_and_zero_band() {
        let fc = FloatingContext::default();
        let r = position_float(
            &fc,
            LayoutFloat::Left,
            size(0.0, 0.0),
            &EdgeSizes::default(),
            0.0,
            0.0,
            HTB,
        );
        assert_eq!(r.origin, LogicalPosition::zero());
        assert_eq!(r.size, size(0.0, 0.0));
    }

    #[test]
    fn position_float_wider_than_the_bfc_does_not_hang() {
        let fc = FloatingContext::default();
        // No float can ever fit; the loop must bail out instead of spinning.
        let left = position_float(
            &fc,
            LayoutFloat::Left,
            size(500.0, 50.0),
            &EdgeSizes::default(),
            0.0,
            300.0,
            HTB,
        );
        assert_eq!(left.origin.x, 0.0);

        let right = position_float(
            &fc,
            LayoutFloat::Right,
            size(500.0, 50.0),
            &EdgeSizes::default(),
            0.0,
            300.0,
            HTB,
        );
        // Overflows the cross-start edge, but deterministically so.
        assert_eq!(right.origin.x, -200.0);
    }

    #[test]
    fn position_float_stacks_then_shifts_down_past_the_lowest_float() {
        let mut fc = FloatingContext::default();
        fc.add_float(
            LayoutFloat::Left,
            rect(0.0, 0.0, 100.0, 50.0),
            EdgeSizes::default(),
        );

        // Fits next to the existing float.
        let beside = position_float(
            &fc,
            LayoutFloat::Left,
            size(100.0, 50.0),
            &EdgeSizes::default(),
            0.0,
            300.0,
            HTB,
        );
        assert_eq!(beside.origin.x, 100.0);
        assert_eq!(beside.origin.y, 0.0);

        // Too wide for the remaining 200px -> must drop below the float's bottom edge.
        let below = position_float(
            &fc,
            LayoutFloat::Left,
            size(250.0, 50.0),
            &EdgeSizes::default(),
            0.0,
            300.0,
            HTB,
        );
        assert_eq!(below.origin.x, 0.0);
        assert_eq!(below.origin.y, 50.0);
    }

    #[test]
    fn position_float_terminates_on_nan_and_infinite_sizes() {
        let mut fc = FloatingContext::default();
        fc.add_float(
            LayoutFloat::Left,
            rect(0.0, 0.0, 100.0, 50.0),
            EdgeSizes::default(),
        );

        // NaN cross size: `available_width >= total_cross` is false, and no float can
        // report a NaN-overlapping band, so the loop exits on the first pass.
        let nan = position_float(
            &fc,
            LayoutFloat::Left,
            size(f32::NAN, f32::NAN),
            &EdgeSizes::default(),
            0.0,
            300.0,
            HTB,
        );
        assert!(nan.size.width.is_nan());
        assert!(nan.origin.x.is_finite());

        // Infinite size: never fits, drops past the one float, then bails out.
        let inf = position_float(
            &fc,
            LayoutFloat::Left,
            size(f32::INFINITY, f32::INFINITY),
            &EdgeSizes::default(),
            0.0,
            300.0,
            HTB,
        );
        assert_eq!(inf.origin.x, 0.0);
    }

    #[test]
    fn position_float_honours_vertical_writing_mode() {
        let fc = FloatingContext::default();
        // vertical-rl: main = x, cross = y.
        let r = position_float(
            &fc,
            LayoutFloat::Left,
            size(50.0, 100.0),
            &EdgeSizes::default(),
            20.0,
            300.0,
            VRL,
        );
        assert_eq!(r.origin.x, 20.0); // main offset lands on x
        assert_eq!(r.origin.y, 0.0); // cross-start lands on y
    }

    // ==================================================================
    // position_floated_child (numeric)
    // ==================================================================

    #[test]
    fn position_floated_child_rejects_float_none() {
        let mut fc = FloatingContext::default();
        let c = constraints(size(300.0, 300.0), HTB);
        let out = position_floated_child(
            0,
            size(10.0, 10.0),
            LayoutFloat::None,
            &c,
            rect(0.0, 0.0, 300.0, 300.0),
            0.0,
            &mut fc,
        );
        assert!(matches!(out, Err(LayoutError::PositioningFailed)));
        assert!(fc.floats.is_empty());
    }

    #[test]
    fn position_floated_child_places_and_records_the_float() {
        let mut fc = FloatingContext::default();
        let c = constraints(size(300.0, 300.0), HTB);

        let left = position_floated_child(
            0,
            size(100.0, 50.0),
            LayoutFloat::Left,
            &c,
            rect(0.0, 0.0, 300.0, 300.0),
            0.0,
            &mut fc,
        )
        .expect("fits");
        assert_eq!(left, LogicalPosition::new(0.0, 0.0));
        assert_eq!(fc.floats.len(), 1);

        let right = position_floated_child(
            1,
            size(100.0, 50.0),
            LayoutFloat::Right,
            &c,
            rect(0.0, 0.0, 300.0, 300.0),
            0.0,
            &mut fc,
        )
        .expect("fits");
        assert_eq!(right, LogicalPosition::new(200.0, 0.0));
        assert_eq!(fc.floats.len(), 2);

        // Third float is wider than the 100px gap left between them -> pushed below.
        let third = position_floated_child(
            2,
            size(150.0, 50.0),
            LayoutFloat::Left,
            &c,
            rect(0.0, 0.0, 300.0, 300.0),
            0.0,
            &mut fc,
        )
        .expect("drops to the next band");
        assert_eq!(third, LogicalPosition::new(0.0, 50.0));
    }

    #[test]
    fn position_floated_child_errors_instead_of_looping_when_nothing_can_fit() {
        let mut fc = FloatingContext::default();
        let c = constraints(size(300.0, 300.0), HTB);
        // Wider than the BFC with no float to drop past -> unrecoverable, must Err.
        let out = position_floated_child(
            0,
            size(500.0, 50.0),
            LayoutFloat::Left,
            &c,
            rect(0.0, 0.0, 300.0, 300.0),
            0.0,
            &mut fc,
        );
        assert!(matches!(out, Err(LayoutError::PositioningFailed)));
        assert!(fc.floats.is_empty());

        // Same for a NaN-sized child: it never "fits", and NaN < float_end is false.
        let nan = position_floated_child(
            0,
            size(f32::NAN, f32::NAN),
            LayoutFloat::Left,
            &c,
            rect(0.0, 0.0, 300.0, 300.0),
            0.0,
            &mut fc,
        );
        assert!(matches!(nan, Err(LayoutError::PositioningFailed)));
    }

    // ==================================================================
    // taffy translation (round-trip)
    // ==================================================================

    #[test]
    fn translate_taffy_size_round_trips_through_taffy() {
        for s in [
            size(0.0, 0.0),
            size(-0.0, 1.5),
            size(f32::MIN, f32::MAX),
            size(-1.0, -2.0),
            size(f32::MIN_POSITIVE, f32::EPSILON),
        ] {
            let t = translate_taffy_size(s);
            let back = translate_taffy_size_back(TaffySize {
                width: t.width.unwrap(),
                height: t.height.unwrap(),
            });
            assert_eq!(back.width.to_bits(), s.width.to_bits(), "width for {s:?}");
            assert_eq!(
                back.height.to_bits(),
                s.height.to_bits(),
                "height for {s:?}"
            );
        }
    }

    #[test]
    fn a_non_finite_parent_size_reaches_taffy_as_unknown() {
        for s in [
            size(f32::NAN, f32::NAN),
            size(f32::INFINITY, f32::NEG_INFINITY),
        ] {
            let t = translate_taffy_size(s);
            assert_eq!((t.width, t.height), (None, None), "{s:?}");
        }
        let half = translate_taffy_size(size(800.0, f32::INFINITY));
        assert_eq!((half.width, half.height), (Some(800.0), None));
    }

    #[test]
    fn translate_taffy_point_back_is_a_field_for_field_copy() {
        for (x, y) in [
            (0.0_f32, 0.0_f32),
            (f32::MIN, f32::MAX),
            (-1.5, 2.5),
            (f32::INFINITY, f32::NEG_INFINITY),
        ] {
            let p = translate_taffy_point_back(taffy::Point { x, y });
            assert_eq!(p.x.to_bits(), x.to_bits());
            assert_eq!(p.y.to_bits(), y.to_bits());
        }
        let nan = translate_taffy_point_back(taffy::Point {
            x: f32::NAN,
            y: f32::NAN,
        });
        assert!(nan.x.is_nan() && nan.y.is_nan());
    }

    // ==================================================================
    // resolve_size_metric (numeric)
    // ==================================================================

    fn resolve(metric: SizeMetric, value: f32) -> f32 {
        resolve_size_metric(metric, value, 200.0, size(1000.0, 500.0), 16.0, 10.0)
    }

    /// The unit conversions divide before multiplying, so they are only exact to
    /// within f32 rounding — compare with a tolerance rather than bit-for-bit.
    fn approx(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 0.001,
            "expected ~{expected}, got {actual}"
        );
    }

    #[test]
    fn resolve_size_metric_converts_every_unit() {
        assert_eq!(resolve(SizeMetric::Px, 42.0), 42.0);
        approx(resolve(SizeMetric::Pt, 72.0), 96.0); // 72pt == 96px
        assert_eq!(resolve(SizeMetric::Percent, 50.0), 100.0); // 50% of 200
        assert_eq!(resolve(SizeMetric::Em, 2.0), 32.0); // 2 * 16px
        assert_eq!(resolve(SizeMetric::Rem, 2.0), 20.0); // 2 * 10px root
        approx(resolve(SizeMetric::Vw, 10.0), 100.0); // 10% of 1000
        approx(resolve(SizeMetric::Vh, 10.0), 50.0); // 10% of 500
        assert_eq!(resolve(SizeMetric::Vmin, 100.0), 500.0); // smaller axis
        assert_eq!(resolve(SizeMetric::Vmax, 100.0), 1000.0); // larger axis
        assert_eq!(resolve(SizeMetric::In, 1.0), 96.0);
        approx(resolve(SizeMetric::Cm, 2.54), 96.0);
        approx(resolve(SizeMetric::Mm, 25.4), 96.0);
    }

    #[test]
    fn resolve_size_metric_at_zero_and_negative_values() {
        for m in [
            SizeMetric::Px,
            SizeMetric::Pt,
            SizeMetric::Percent,
            SizeMetric::Em,
            SizeMetric::Rem,
            SizeMetric::Vw,
            SizeMetric::Vh,
            SizeMetric::Vmin,
            SizeMetric::Vmax,
            SizeMetric::In,
            SizeMetric::Cm,
            SizeMetric::Mm,
        ] {
            assert_eq!(resolve(m, 0.0), 0.0, "{m:?} at zero");
            assert!(
                resolve(m, -10.0) <= 0.0,
                "{m:?} keeps the sign of a negative"
            );
        }
        // Percentages of a negative containing block stay negative.
        assert_eq!(
            resolve_size_metric(
                SizeMetric::Percent,
                50.0,
                -200.0,
                size(0.0, 0.0),
                16.0,
                16.0
            ),
            -100.0
        );
    }

    #[test]
    fn resolve_size_metric_saturates_instead_of_panicking_at_the_limits() {
        let huge = resolve_size_metric(
            SizeMetric::Em,
            f32::MAX,
            0.0,
            size(0.0, 0.0),
            f32::MAX,
            16.0,
        );
        assert!(huge.is_infinite(), "MAX * MAX saturates to +inf");

        let neg = resolve_size_metric(
            SizeMetric::Percent,
            f32::MIN,
            f32::MAX,
            size(0.0, 0.0),
            16.0,
            16.0,
        );
        assert!(neg.is_infinite() && neg.is_sign_negative());
    }

    #[test]
    fn resolve_size_metric_propagates_nan_and_inf_without_panicking() {
        assert!(resolve(SizeMetric::Px, f32::NAN).is_nan());
        assert!(resolve(SizeMetric::Px, f32::INFINITY).is_infinite());
        assert!(resolve(SizeMetric::Percent, f32::NAN).is_nan());
        // 0 * inf is the classic NaN trap: it must not panic, it just yields NaN.
        assert!(resolve_size_metric(
            SizeMetric::Em,
            0.0,
            0.0,
            size(0.0, 0.0),
            f32::INFINITY,
            16.0
        )
        .is_nan());
        // f32::min/max drop NaN, so a half-NaN viewport still resolves vmin/vmax.
        let vp = size(f32::NAN, 400.0);
        assert_eq!(
            resolve_size_metric(SizeMetric::Vmin, 100.0, 0.0, vp, 16.0, 16.0),
            400.0
        );
        assert_eq!(
            resolve_size_metric(SizeMetric::Vmax, 100.0, 0.0, vp, 16.0, 16.0),
            400.0
        );
    }

    // ==================================================================
    // convert_font_style / convert_font_weight (other)
    // ==================================================================

    #[test]
    fn convert_font_style_maps_every_variant() {
        assert_eq!(
            convert_font_style(StyleFontStyle::Normal),
            crate::font_traits::FontStyle::Normal
        );
        assert_eq!(
            convert_font_style(StyleFontStyle::Italic),
            crate::font_traits::FontStyle::Italic
        );
        assert_eq!(
            convert_font_style(StyleFontStyle::Oblique),
            crate::font_traits::FontStyle::Oblique
        );
    }

    #[test]
    fn convert_font_weight_maps_every_variant_monotonically() {
        assert_eq!(convert_font_weight(StyleFontWeight::W100), FcWeight::Thin);
        assert_eq!(
            convert_font_weight(StyleFontWeight::W200),
            FcWeight::ExtraLight
        );
        assert_eq!(convert_font_weight(StyleFontWeight::W300), FcWeight::Light);
        assert_eq!(
            convert_font_weight(StyleFontWeight::Lighter),
            FcWeight::Light
        );
        assert_eq!(
            convert_font_weight(StyleFontWeight::Normal),
            FcWeight::Normal
        );
        assert_eq!(convert_font_weight(StyleFontWeight::W500), FcWeight::Medium);
        assert_eq!(
            convert_font_weight(StyleFontWeight::W600),
            FcWeight::SemiBold
        );
        assert_eq!(convert_font_weight(StyleFontWeight::Bold), FcWeight::Bold);
        assert_eq!(
            convert_font_weight(StyleFontWeight::W800),
            FcWeight::ExtraBold
        );
        assert_eq!(convert_font_weight(StyleFontWeight::W900), FcWeight::Black);
        assert_eq!(
            convert_font_weight(StyleFontWeight::Bolder),
            FcWeight::Black
        );
    }

    // ==================================================================
    // BorderInfo (other)
    // ==================================================================

    fn bi(width: f32, style: BorderStyle, source: BorderSource) -> BorderInfo {
        BorderInfo::new(width, style, ColorU::BLACK, source)
    }

    #[test]
    fn border_info_new_stores_its_arguments_verbatim() {
        let b = BorderInfo::new(
            f32::NAN,
            BorderStyle::Dotted,
            ColorU::RED,
            BorderSource::Cell,
        );
        assert!(b.width.is_nan());
        assert_eq!(b.style, BorderStyle::Dotted);
        assert_eq!(b.color, ColorU::RED);
        assert_eq!(b.source, BorderSource::Cell);
    }

    #[test]
    fn border_style_priority_follows_the_css_ordering() {
        assert_eq!(BorderInfo::style_priority(&BorderStyle::Hidden), 255);
        assert_eq!(BorderInfo::style_priority(&BorderStyle::None), 0);
        // double > solid > dashed > dotted > ridge > outset > groove > inset
        let ordered = [
            BorderStyle::Double,
            BorderStyle::Solid,
            BorderStyle::Dashed,
            BorderStyle::Dotted,
            BorderStyle::Ridge,
            BorderStyle::Outset,
            BorderStyle::Groove,
            BorderStyle::Inset,
        ];
        for w in ordered.windows(2) {
            assert!(
                BorderInfo::style_priority(&w[0]) > BorderInfo::style_priority(&w[1]),
                "{:?} must outrank {:?}",
                w[0],
                w[1]
            );
        }
    }

    #[test]
    fn resolve_conflict_hidden_suppresses_everything() {
        let hidden = bi(1.0, BorderStyle::Hidden, BorderSource::Table);
        let fat = bi(99.0, BorderStyle::Solid, BorderSource::Cell);
        assert!(BorderInfo::resolve_conflict(&hidden, &fat).is_none());
        assert!(BorderInfo::resolve_conflict(&fat, &hidden).is_none());
    }

    #[test]
    fn resolve_conflict_none_always_loses() {
        let none = bi(50.0, BorderStyle::None, BorderSource::Cell);
        let thin = bi(1.0, BorderStyle::Solid, BorderSource::Table);
        assert!(BorderInfo::resolve_conflict(&none, &none).is_none());
        // 'none' loses even when it is much wider.
        assert_eq!(
            BorderInfo::resolve_conflict(&none, &thin).unwrap().style,
            BorderStyle::Solid
        );
        assert_eq!(
            BorderInfo::resolve_conflict(&thin, &none).unwrap().style,
            BorderStyle::Solid
        );
    }

    #[test]
    fn resolve_conflict_falls_through_width_then_style_then_source() {
        let wide = bi(5.0, BorderStyle::Dotted, BorderSource::Table);
        let narrow = bi(2.0, BorderStyle::Double, BorderSource::Cell);
        assert_eq!(
            BorderInfo::resolve_conflict(&wide, &narrow).unwrap().width,
            5.0
        );

        // Same width -> style priority decides (double beats dotted).
        let a = bi(3.0, BorderStyle::Dotted, BorderSource::Cell);
        let b = bi(3.0, BorderStyle::Double, BorderSource::Table);
        assert_eq!(
            BorderInfo::resolve_conflict(&a, &b).unwrap().style,
            BorderStyle::Double
        );

        // Same width + style -> source priority decides (cell beats table).
        let t = bi(3.0, BorderStyle::Solid, BorderSource::Table);
        let c = bi(3.0, BorderStyle::Solid, BorderSource::Cell);
        assert_eq!(
            BorderInfo::resolve_conflict(&t, &c).unwrap().source,
            BorderSource::Cell
        );
        assert_eq!(
            BorderInfo::resolve_conflict(&c, &t).unwrap().source,
            BorderSource::Cell
        );

        // Fully tied -> the first (left/top) wins.
        let first = BorderInfo::new(3.0, BorderStyle::Solid, ColorU::RED, BorderSource::Row);
        let second = BorderInfo::new(3.0, BorderStyle::Solid, ColorU::GREEN, BorderSource::Row);
        assert_eq!(
            BorderInfo::resolve_conflict(&first, &second).unwrap().color,
            ColorU::RED
        );
    }

    #[test]
    fn resolve_conflict_with_nan_widths_is_deterministic() {
        // Neither `a.width > b.width` nor `b.width > a.width` holds for NaN, so the
        // algorithm must fall through to style/source rather than panic or hang.
        let nan = bi(f32::NAN, BorderStyle::Solid, BorderSource::Table);
        let ok = bi(1.0, BorderStyle::Solid, BorderSource::Cell);
        let win = BorderInfo::resolve_conflict(&nan, &ok).unwrap();
        assert_eq!(win.source, BorderSource::Cell); // source breaks the tie
        assert_eq!(win.width, 1.0);

        // Equal source too -> first argument wins, NaN width and all.
        let nan_b = bi(f32::NAN, BorderStyle::Solid, BorderSource::Table);
        let other = bi(2.0, BorderStyle::Solid, BorderSource::Table);
        assert!(BorderInfo::resolve_conflict(&nan_b, &other)
            .unwrap()
            .width
            .is_nan());
    }

    #[test]
    fn resolve_conflict_infinite_width_wins() {
        let inf = bi(f32::INFINITY, BorderStyle::Inset, BorderSource::Table);
        let solid = bi(f32::MAX, BorderStyle::Solid, BorderSource::Cell);
        assert!(BorderInfo::resolve_conflict(&inf, &solid)
            .unwrap()
            .width
            .is_infinite());
    }

    // ==================================================================
    // is_cell_empty / is_empty_block / compute_cell_baseline
    // ==================================================================

    #[test]
    fn is_cell_empty_treats_missing_and_childless_cells_as_empty() {
        let tree = build_tree(
            vec![hot(None, Some(size(10.0, 10.0)), &BoxProps::default())],
            vec![LayoutNodeWarm::default()],
            &[vec![]],
        );
        assert!(is_cell_empty(&tree, 0), "no children => empty");
        assert!(is_cell_empty(&tree, 1), "out-of-range index => empty");
        assert!(
            is_cell_empty(&tree, usize::MAX),
            "usize::MAX must not panic"
        );
    }

    #[test]
    fn is_cell_empty_uses_the_inline_layout_when_present() {
        let bp = BoxProps::default();
        let mut warm = vec![LayoutNodeWarm::default(), LayoutNodeWarm::default()];
        // Cell (0) has a child (1) but an inline layout with no items => empty.
        warm[0].inline_layout_result = Some(Box::new(empty_inline_layout()));
        let tree = build_tree(
            vec![
                hot(None, Some(size(10.0, 10.0)), &bp),
                hot(Some(0), Some(size(10.0, 10.0)), &bp),
            ],
            warm,
            &[vec![1], vec![]],
        );
        assert!(is_cell_empty(&tree, 0));

        // Same tree without the inline layout: children alone mean "not empty".
        let tree2 = build_tree(
            vec![
                hot(None, Some(size(10.0, 10.0)), &bp),
                hot(Some(0), Some(size(10.0, 10.0)), &bp),
            ],
            vec![LayoutNodeWarm::default(), LayoutNodeWarm::default()],
            &[vec![1], vec![]],
        );
        assert!(!is_cell_empty(&tree2, 0));
    }

    #[test]
    fn is_empty_block_requires_no_children_no_inline_content_and_no_height() {
        let bp = BoxProps::default();

        // Missing node => vacuously empty.
        let empty_tree = build_tree(Vec::new(), Vec::new(), &[]);
        assert!(is_empty_block(&empty_tree, 0));
        assert!(is_empty_block(&empty_tree, usize::MAX));

        // No children, no used_size => empty.
        let t = build_tree(
            vec![hot(None, None, &bp)],
            vec![LayoutNodeWarm::default()],
            &[vec![]],
        );
        assert!(is_empty_block(&t, 0));

        // Zero and negative heights still count as empty (the check is `> 0.0`).
        for h in [0.0, -5.0] {
            let t = build_tree(
                vec![hot(None, Some(size(100.0, h)), &bp)],
                vec![LayoutNodeWarm::default()],
                &[vec![]],
            );
            assert!(is_empty_block(&t, 0), "height {h} must count as empty");
        }

        // A positive height makes it non-empty.
        let t = build_tree(
            vec![hot(None, Some(size(100.0, 0.5)), &bp)],
            vec![LayoutNodeWarm::default()],
            &[vec![]],
        );
        assert!(!is_empty_block(&t, 0));

        // Children make it non-empty.
        let t = build_tree(
            vec![hot(None, None, &bp), hot(Some(0), None, &bp)],
            vec![LayoutNodeWarm::default(), LayoutNodeWarm::default()],
            &[vec![1], vec![]],
        );
        assert!(!is_empty_block(&t, 0));

        // An inline layout result makes it non-empty even if it has no items.
        let mut warm = vec![LayoutNodeWarm::default()];
        warm[0].inline_layout_result = Some(Box::new(empty_inline_layout()));
        let t = build_tree(vec![hot(None, None, &bp)], warm, &[vec![]]);
        assert!(!is_empty_block(&t, 0));
    }

    #[test]
    fn compute_cell_baseline_falls_back_to_the_content_edge() {
        let bp = box_props(
            EdgeSizes::default(),
            edges(0.0, 0.0, 2.0, 0.0), // border-bottom: 2
            edges(0.0, 0.0, 5.0, 0.0), // padding-bottom: 5
        );
        let tree = build_tree(
            vec![hot(None, Some(size(50.0, 100.0)), &bp)],
            vec![LayoutNodeWarm::default()],
            &[vec![]],
        );
        // No line box: baseline == bottom of the content edge (100 - 5 - 2).
        assert_eq!(compute_cell_baseline(0, &tree), 93.0);

        // Missing node => 0.0, never a panic.
        assert_eq!(compute_cell_baseline(usize::MAX, &tree), 0.0);
    }

    #[test]
    fn compute_cell_baseline_without_used_size_can_go_negative() {
        let bp = box_props(
            EdgeSizes::default(),
            edges(0.0, 0.0, 2.0, 0.0),
            edges(0.0, 0.0, 5.0, 0.0),
        );
        let tree = build_tree(
            vec![hot(None, None, &bp)],
            vec![LayoutNodeWarm::default()],
            &[vec![]],
        );
        // used_size defaults to 0x0, so the baseline is -(padding + border).
        assert_eq!(compute_cell_baseline(0, &tree), -7.0);
    }

    // ==================================================================
    // check_scrollbar_necessity (numeric)
    // ==================================================================

    #[test]
    fn check_scrollbar_necessity_never_scrolls_for_visible_hidden_or_clip() {
        for o in [
            OverflowBehavior::Visible,
            OverflowBehavior::Hidden,
            OverflowBehavior::Clip,
        ] {
            let r = check_scrollbar_necessity(size(9999.0, 9999.0), size(10.0, 10.0), o, o, 16.0);
            assert!(!r.needs_horizontal, "{o:?}");
            assert!(!r.needs_vertical, "{o:?}");
            assert_eq!(r.scrollbar_width, 0.0);
            assert_eq!(r.scrollbar_height, 0.0);
        }
    }

    #[test]
    fn check_scrollbar_necessity_always_scrolls_for_scroll_even_with_no_content() {
        let r = check_scrollbar_necessity(
            size(0.0, 0.0),
            size(500.0, 500.0),
            OverflowBehavior::Scroll,
            OverflowBehavior::Scroll,
            16.0,
        );
        assert!(r.needs_horizontal && r.needs_vertical);
        assert_eq!(r.scrollbar_width, 16.0);
        assert_eq!(r.scrollbar_height, 16.0);
    }

    #[test]
    fn check_scrollbar_necessity_auto_honours_the_one_pixel_epsilon() {
        let auto = OverflowBehavior::Auto;
        // Exactly at the epsilon boundary: 301 is NOT > 300 + 1.
        let r = check_scrollbar_necessity(size(301.0, 301.0), size(300.0, 300.0), auto, auto, 0.0);
        assert!(!r.needs_horizontal);
        assert!(!r.needs_vertical);

        // One ulp past the boundary triggers both.
        let r = check_scrollbar_necessity(size(302.0, 302.0), size(300.0, 300.0), auto, auto, 0.0);
        assert!(r.needs_horizontal && r.needs_vertical);
        // Overlay scrollbars: needed, but they reserve no layout space.
        assert_eq!(r.scrollbar_width, 0.0);
        assert_eq!(r.scrollbar_height, 0.0);
    }

    #[test]
    fn check_scrollbar_necessity_two_pass_adds_the_second_scrollbar() {
        let auto = OverflowBehavior::Auto;
        // Vertically overflowing; horizontally it fits exactly — until the 16px
        // vertical scrollbar eats into the width.
        let r = check_scrollbar_necessity(size(300.0, 400.0), size(300.0, 300.0), auto, auto, 16.0);
        assert!(r.needs_vertical);
        assert!(
            r.needs_horizontal,
            "vertical scrollbar must force a horizontal one"
        );
        assert_eq!(r.scrollbar_width, 16.0);
        assert_eq!(r.scrollbar_height, 16.0);

        // With overlay scrollbars (0px) the second pass is skipped.
        let r = check_scrollbar_necessity(size(300.0, 400.0), size(300.0, 300.0), auto, auto, 0.0);
        assert!(r.needs_vertical);
        assert!(!r.needs_horizontal);
    }

    #[test]
    fn check_scrollbar_necessity_with_nan_and_negative_sizes_is_deterministic() {
        let auto = OverflowBehavior::Auto;
        // NaN comparisons are false => no scrollbars, no panic.
        let r = check_scrollbar_necessity(
            size(f32::NAN, f32::NAN),
            size(300.0, 300.0),
            auto,
            auto,
            16.0,
        );
        assert!(!r.needs_horizontal && !r.needs_vertical);

        // A NaN container is equally inert.
        let r = check_scrollbar_necessity(
            size(500.0, 500.0),
            size(f32::NAN, f32::NAN),
            auto,
            auto,
            16.0,
        );
        assert!(!r.needs_horizontal && !r.needs_vertical);

        // Negative container sizes: content trivially overflows, both appear.
        let r = check_scrollbar_necessity(size(0.0, 0.0), size(-100.0, -100.0), auto, auto, 16.0);
        assert!(r.needs_horizontal && r.needs_vertical);

        // Infinite content overflows anything finite.
        let r = check_scrollbar_necessity(
            size(f32::INFINITY, f32::INFINITY),
            size(300.0, 300.0),
            auto,
            auto,
            16.0,
        );
        assert!(r.needs_horizontal && r.needs_vertical);
    }

    #[test]
    fn check_scrollbar_necessity_with_negative_scrollbar_width_skips_the_second_pass() {
        let auto = OverflowBehavior::Auto;
        let r =
            check_scrollbar_necessity(size(300.0, 400.0), size(300.0, 300.0), auto, auto, -16.0);
        assert!(r.needs_vertical);
        assert!(
            !r.needs_horizontal,
            "the `> 0.0` guard skips the two-pass check"
        );
        assert_eq!(r.scrollbar_width, -16.0); // passed through verbatim
    }

    // ==================================================================
    // collapse_margins / advance_pen_with_margin_collapse (numeric)
    // ==================================================================

    #[test]
    fn collapse_margins_follows_css_2_1_section_8_3_1() {
        assert_eq!(collapse_margins(10.0, 20.0), 20.0); // both positive -> max
        assert_eq!(collapse_margins(-10.0, -20.0), -20.0); // both negative -> min
        assert_eq!(collapse_margins(20.0, -5.0), 15.0); // mixed -> sum
        assert_eq!(collapse_margins(-5.0, 20.0), 15.0);
        assert_eq!(collapse_margins(0.0, 0.0), 0.0);
        assert_eq!(collapse_margins(0.0, 10.0), 10.0);
        // +0.0 counts as positive, so a zero/negative pair is summed.
        assert_eq!(collapse_margins(0.0, -5.0), -5.0);
    }

    #[test]
    fn collapse_margins_is_commutative_for_finite_inputs() {
        let vals = [
            -100.0_f32,
            -1.0,
            -0.5,
            0.0,
            0.5,
            1.0,
            100.0,
            f32::MAX,
            f32::MIN,
        ];
        for a in vals {
            for b in vals {
                let ab = collapse_margins(a, b);
                let ba = collapse_margins(b, a);
                assert_eq!(ab.to_bits(), ba.to_bits(), "collapse_margins({a}, {b})");
            }
        }
    }

    #[test]
    fn collapse_margins_saturates_and_defines_nan_inf_behaviour() {
        assert!(collapse_margins(f32::MAX, -f32::MAX).abs() < 1.0); // sum, no overflow
        assert_eq!(collapse_margins(f32::INFINITY, 5.0), f32::INFINITY);
        assert_eq!(collapse_margins(f32::NEG_INFINITY, -5.0), f32::NEG_INFINITY);
        // +inf and -inf have mixed signs -> summed -> NaN (must not panic).
        assert!(collapse_margins(f32::INFINITY, f32::NEG_INFINITY).is_nan());
        // f32::max/min discard NaN, so a NaN margin is ignored rather than propagated.
        assert_eq!(collapse_margins(f32::NAN, 1.0), 1.0);
        assert_eq!(collapse_margins(1.0, f32::NAN), 1.0);
        assert_eq!(collapse_margins(-f32::NAN, -1.0), -1.0);
    }

    #[test]
    fn advance_pen_with_margin_collapse_advances_by_exactly_the_collapsed_margin() {
        let mut pen = 100.0_f32;
        let collapsed = advance_pen_with_margin_collapse(&mut pen, 10.0, 20.0);
        assert_eq!(collapsed, 20.0);
        assert_eq!(pen, 120.0);

        // Mixed signs pull the pen back up.
        let mut pen = 100.0_f32;
        let collapsed = advance_pen_with_margin_collapse(&mut pen, 30.0, -10.0);
        assert_eq!(collapsed, 20.0);
        assert_eq!(pen, 120.0);

        // Zero margins leave the pen alone.
        let mut pen = 7.5_f32;
        assert_eq!(advance_pen_with_margin_collapse(&mut pen, 0.0, 0.0), 0.0);
        assert_eq!(pen, 7.5);
    }

    #[test]
    fn advance_pen_with_margin_collapse_saturates_at_the_f32_limits() {
        let mut pen = f32::MAX;
        let collapsed = advance_pen_with_margin_collapse(&mut pen, f32::MAX, f32::MAX);
        assert_eq!(collapsed, f32::MAX);
        assert!(pen.is_infinite(), "MAX + MAX saturates to +inf, no panic");

        let mut pen = f32::NAN;
        let collapsed = advance_pen_with_margin_collapse(&mut pen, 1.0, 2.0);
        assert_eq!(collapsed, 2.0);
        assert!(pen.is_nan(), "a NaN pen stays NaN");
    }

    // ==================================================================
    // has_margin_collapse_blocker (predicate)
    // ==================================================================

    #[test]
    fn has_margin_collapse_blocker_basic_true_false() {
        let none = BoxProps::default();
        assert!(!has_margin_collapse_blocker(&none, HTB, true));
        assert!(!has_margin_collapse_blocker(&none, HTB, false));

        let top_border = box_props(
            EdgeSizes::default(),
            edges(1.0, 0.0, 0.0, 0.0),
            EdgeSizes::default(),
        );
        assert!(has_margin_collapse_blocker(&top_border, HTB, true));
        assert!(!has_margin_collapse_blocker(&top_border, HTB, false));

        let bottom_padding = box_props(
            EdgeSizes::default(),
            EdgeSizes::default(),
            edges(0.0, 0.0, 0.5, 0.0),
        );
        assert!(!has_margin_collapse_blocker(&bottom_padding, HTB, true));
        assert!(has_margin_collapse_blocker(&bottom_padding, HTB, false));
    }

    #[test]
    fn has_margin_collapse_blocker_maps_edges_per_writing_mode() {
        // In vertical writing modes the main axis is horizontal: left/right block.
        let left_border = box_props(
            EdgeSizes::default(),
            edges(0.0, 0.0, 0.0, 3.0),
            EdgeSizes::default(),
        );
        assert!(has_margin_collapse_blocker(&left_border, VRL, true));
        assert!(!has_margin_collapse_blocker(&left_border, VRL, false));
        // The same box does not block in horizontal-tb (left is a cross edge there).
        assert!(!has_margin_collapse_blocker(&left_border, HTB, true));
        assert!(!has_margin_collapse_blocker(&left_border, HTB, false));
    }

    #[test]
    fn has_margin_collapse_blocker_ignores_negative_and_nan_edges() {
        let weird = box_props(
            EdgeSizes::default(),
            edges(-5.0, 0.0, f32::NAN, 0.0),
            edges(f32::NAN, 0.0, -1.0, 0.0),
        );
        // `> 0.0` is false for both negatives and NaN -> nothing blocks, no panic.
        assert!(!has_margin_collapse_blocker(&weird, HTB, true));
        assert!(!has_margin_collapse_blocker(&weird, HTB, false));

        // Infinity does block.
        let inf = box_props(
            EdgeSizes::default(),
            edges(f32::INFINITY, 0.0, 0.0, 0.0),
            EdgeSizes::default(),
        );
        assert!(has_margin_collapse_blocker(&inf, HTB, true));
    }

    // ==================================================================
    // is_bk_or_nl_class / is_bidi_control / is_css_document_whitespace
    // ==================================================================

    #[test]
    fn is_bk_or_nl_class_covers_exactly_vt_ff_nel_ls_ps() {
        for c in ['\u{000B}', '\u{000C}', '\u{0085}', '\u{2028}', '\u{2029}'] {
            assert!(is_bk_or_nl_class(c), "{:04X} must be BK/NL", c as u32);
        }
        // LF and CR are handled separately by the callers, not by this predicate.
        for c in ['\n', '\r', ' ', '\t', 'a', '\0', '\u{200B}', char::MAX] {
            assert!(!is_bk_or_nl_class(c), "{:04X} must not be BK/NL", c as u32);
        }
    }

    #[test]
    fn is_bidi_control_covers_the_uax9_set_only() {
        for c in [
            '\u{200E}', '\u{200F}', '\u{202A}', '\u{202B}', '\u{202C}', '\u{202D}', '\u{202E}',
            '\u{2066}', '\u{2067}', '\u{2068}', '\u{2069}', '\u{061C}',
        ] {
            assert!(
                is_bidi_control(c),
                "{:04X} must be a bidi control",
                c as u32
            );
        }
        for c in [
            'a',
            ' ',
            '\u{200B}',
            '\u{2028}',
            '\u{202F}',
            '\u{2065}',
            char::MAX,
        ] {
            assert!(
                !is_bidi_control(c),
                "{:04X} must not be a bidi control",
                c as u32
            );
        }
    }

    #[test]
    fn is_css_document_whitespace_excludes_other_unicode_spaces() {
        for c in [' ', '\t', '\n', '\r', '\x0C'] {
            assert!(is_css_document_whitespace(c));
        }
        // NBSP, ideographic space, ZWSP, LS and VT are NOT document white space.
        for c in [
            '\u{00A0}', '\u{3000}', '\u{200B}', '\u{2028}', '\u{000B}', 'a',
        ] {
            assert!(
                !is_css_document_whitespace(c),
                "{:04X} must not be document white space",
                c as u32
            );
        }
    }

    // ==================================================================
    // split_at_forced_breaks / split_at_bk_nl_chars (other)
    // ==================================================================

    #[test]
    fn split_at_forced_breaks_handles_every_newline_flavour() {
        assert_eq!(split_at_forced_breaks(""), vec![String::new()]);
        assert_eq!(split_at_forced_breaks("abc"), vec!["abc".to_string()]);
        assert_eq!(split_at_forced_breaks("a\nb"), vec!["a", "b"]);
        assert_eq!(split_at_forced_breaks("a\r\nb"), vec!["a", "b"]);
        assert_eq!(split_at_forced_breaks("a\rb"), vec!["a", "b"]);
        // A lone \r followed by another \r is two breaks, not one.
        assert_eq!(split_at_forced_breaks("a\r\rb"), vec!["a", "", "b"]);
        // Leading / trailing breaks produce empty leading / trailing segments.
        assert_eq!(split_at_forced_breaks("\na\n"), vec!["", "a", ""]);
        // BK/NL class chars break too.
        assert_eq!(split_at_forced_breaks("a\u{2028}b"), vec!["a", "b"]);
        assert_eq!(split_at_forced_breaks("a\u{000B}b"), vec!["a", "b"]);
    }

    #[test]
    fn split_at_forced_breaks_preserves_every_non_break_char() {
        let text = "héllo 🌍\u{0301}\u{200B}x";
        let segs = split_at_forced_breaks(text);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0], text, "no char-boundary slicing damage");

        // The number of segments is always (number of breaks) + 1.
        let many = "a\nb\rc\r\nd\u{2029}e";
        assert_eq!(split_at_forced_breaks(many).len(), 5);
        assert_eq!(split_at_forced_breaks(many).concat(), "abcde");
    }

    #[test]
    fn split_at_forced_breaks_survives_a_huge_all_break_input() {
        let text = "\n".repeat(10_000);
        let segs = split_at_forced_breaks(&text);
        assert_eq!(segs.len(), 10_001);
        assert!(segs.iter().all(String::is_empty));
    }

    #[test]
    fn split_at_bk_nl_chars_ignores_lf_and_cr() {
        assert_eq!(split_at_bk_nl_chars(""), vec![String::new()]);
        // \n and \r are collapsible in normal/nowrap, so they are NOT split points.
        assert_eq!(split_at_bk_nl_chars("a\nb\rc"), vec!["a\nb\rc".to_string()]);
        // BK/NL class chars still force a break.
        assert_eq!(split_at_bk_nl_chars("a\u{0085}b"), vec!["a", "b"]);
        assert_eq!(split_at_bk_nl_chars("\u{2029}"), vec!["", ""]);
    }

    // ==================================================================
    // is_east_asian_wide / is_east_asian_fullwidth_or_wide (predicates)
    // ==================================================================

    #[test]
    fn is_east_asian_wide_matches_cjk_kana_and_hangul() {
        for c in ['中', '一', 'あ', 'カ', '。', '가', 'ㄅ', 'Ａ'] {
            assert!(is_east_asian_wide(c), "{:04X} must be wide", c as u32);
        }
        for c in ['a', 'Z', '0', ' ', 'é', '\u{0000}', char::MAX] {
            assert!(!is_east_asian_wide(c), "{:04X} must not be wide", c as u32);
        }
    }

    #[test]
    fn is_east_asian_wide_range_boundaries_are_inclusive() {
        assert!(!is_east_asian_wide('\u{4DFF}')); // just below CJK Unified
        assert!(is_east_asian_wide('\u{4E00}')); // first CJK Unified
        assert!(is_east_asian_wide('\u{9FFF}')); // last CJK Unified
        assert!(!is_east_asian_wide('\u{A000}')); // Yi — not in this list
        assert!(!is_east_asian_wide('\u{FF00}')); // just below fullwidth forms
        assert!(is_east_asian_wide('\u{FF01}')); // first fullwidth form
        assert!(is_east_asian_wide('\u{FF60}')); // last fullwidth form
        assert!(!is_east_asian_wide('\u{FF61}')); // halfwidth forms start here
    }

    #[test]
    fn is_east_asian_fullwidth_or_wide_excludes_hangul_but_wide_does_not() {
        // The two predicates deliberately disagree on Hangul: the segment-break
        // transform must NOT drop breaks between Hangul syllables.
        assert!(is_east_asian_wide('가'));
        assert!(!is_east_asian_fullwidth_or_wide('가'));
        assert!(!is_east_asian_fullwidth_or_wide('\u{1100}')); // Hangul Jamo
        assert!(!is_east_asian_fullwidth_or_wide('\u{3130}')); // Compat Jamo
        assert!(!is_east_asian_fullwidth_or_wide('\u{A960}')); // Jamo Extended-A
        assert!(!is_east_asian_fullwidth_or_wide('\u{D7B0}')); // Jamo Extended-B

        // Han / kana still count, and the halfwidth + Yi ranges are added on top.
        assert!(is_east_asian_fullwidth_or_wide('中'));
        assert!(is_east_asian_fullwidth_or_wide('あ'));
        assert!(is_east_asian_fullwidth_or_wide('\u{FF61}'));
        assert!(is_east_asian_fullwidth_or_wide('\u{A000}'));
        assert!(!is_east_asian_fullwidth_or_wide('a'));
        assert!(!is_east_asian_fullwidth_or_wide(char::MAX));
    }

    // ==================================================================
    // apply_segment_break_transform (other)
    // ==================================================================

    #[test]
    fn apply_segment_break_transform_converts_breaks_to_a_single_space() {
        assert_eq!(apply_segment_break_transform(""), "");
        assert_eq!(apply_segment_break_transform("ab"), "ab");
        assert_eq!(apply_segment_break_transform("a\nb"), "a b");
        assert_eq!(apply_segment_break_transform("a\r\nb"), "a b");
        assert_eq!(apply_segment_break_transform("a\rb"), "a b");
        // §4.1.1: white space around the break is removed first.
        assert_eq!(apply_segment_break_transform("a \n b"), "a b");
        assert_eq!(apply_segment_break_transform("a\t\n\tb"), "a b");
        // Consecutive breaks collapse into one space.
        assert_eq!(apply_segment_break_transform("a\n\n\nb"), "a b");
    }

    #[test]
    fn apply_segment_break_transform_at_the_string_edges() {
        // No char before -> still a space; no char after -> still a space.
        assert_eq!(apply_segment_break_transform("\na"), " a");
        assert_eq!(apply_segment_break_transform("a\n"), "a ");
        assert_eq!(apply_segment_break_transform("\n"), " ");
        assert_eq!(apply_segment_break_transform("  \n  "), " ");
    }

    #[test]
    fn apply_segment_break_transform_removes_breaks_around_zwsp_and_cjk() {
        // Rule 1: adjacent to U+200B ZERO WIDTH SPACE -> the break disappears.
        assert_eq!(apply_segment_break_transform("a\u{200B}\nb"), "a\u{200B}b");
        assert_eq!(apply_segment_break_transform("a\n\u{200B}b"), "a\u{200B}b");
        // Rule 2: East Asian on both sides -> the break disappears.
        assert_eq!(apply_segment_break_transform("中\n文"), "中文");
        assert_eq!(apply_segment_break_transform("あ \n い"), "あい");
        // Only one side East Asian -> a space is kept.
        assert_eq!(apply_segment_break_transform("中\na"), "中 a");
        assert_eq!(apply_segment_break_transform("a\n中"), "a 中");
        // Hangul is excluded from rule 2, so the break becomes a space.
        assert_eq!(apply_segment_break_transform("가\n나"), "가 나");
    }

    #[test]
    fn apply_segment_break_transform_survives_pathological_input() {
        let text = "\n".repeat(5_000);
        assert_eq!(apply_segment_break_transform(&text), " ");
        // Interleaved breaks and multi-byte chars must not slice a char boundary.
        let mixed = "🌍\n🌍\n🌍";
        assert_eq!(apply_segment_break_transform(mixed), "🌍 🌍 🌍");
    }

    // ==================================================================
    // apply_text_transform (other)
    // ==================================================================

    #[test]
    fn apply_text_transform_none_is_the_identity() {
        for s in ["", "abc", "ÄÖÜ", "🌍", " \t\n"] {
            assert_eq!(apply_text_transform(s, TextTransform::None), s);
        }
    }

    #[test]
    fn apply_text_transform_case_changes_handle_growing_and_multibyte_chars() {
        assert_eq!(apply_text_transform("abc", TextTransform::Uppercase), "ABC");
        // ß uppercases to two chars — the output is longer than the input.
        assert_eq!(
            apply_text_transform("straße", TextTransform::Uppercase),
            "STRASSE"
        );
        assert_eq!(apply_text_transform("ÄÖÜ", TextTransform::Lowercase), "äöü");
        assert_eq!(apply_text_transform("", TextTransform::Uppercase), "");
        assert_eq!(apply_text_transform("🌍", TextTransform::Uppercase), "🌍");
    }

    #[test]
    fn apply_text_transform_capitalize_uses_word_boundaries() {
        assert_eq!(
            apply_text_transform("hello world", TextTransform::Capitalize),
            "Hello World"
        );
        // A leading digit is not alphabetic and is not a boundary either.
        assert_eq!(
            apply_text_transform("1st place", TextTransform::Capitalize),
            "1st Place"
        );
        // ASCII punctuation opens a new word.
        assert_eq!(
            apply_text_transform("a-b (c)", TextTransform::Capitalize),
            "A-B (C)"
        );
        assert_eq!(
            apply_text_transform("élan", TextTransform::Capitalize),
            "Élan"
        );
        assert_eq!(apply_text_transform("", TextTransform::Capitalize), "");
    }

    #[test]
    fn apply_text_transform_fullwidth_maps_the_ascii_block() {
        assert_eq!(
            apply_text_transform("a", TextTransform::FullWidth),
            "\u{FF41}"
        );
        assert_eq!(
            apply_text_transform("!", TextTransform::FullWidth),
            "\u{FF01}"
        );
        assert_eq!(
            apply_text_transform("~", TextTransform::FullWidth),
            "\u{FF5E}"
        );
        assert_eq!(
            apply_text_transform(" ", TextTransform::FullWidth),
            "\u{3000}"
        );
        // Chars outside U+0021..U+007E are passed through untouched.
        assert_eq!(
            apply_text_transform("\t\n", TextTransform::FullWidth),
            "\t\n"
        );
        assert_eq!(
            apply_text_transform("\u{7F}", TextTransform::FullWidth),
            "\u{7F}"
        );
        assert_eq!(
            apply_text_transform("中🌍", TextTransform::FullWidth),
            "中🌍"
        );
    }

    // ==================================================================
    // layout_initial_letter (numeric)
    // ==================================================================

    #[test]
    fn layout_initial_letter_rejects_degenerate_parameters() {
        // size <= 0, line_height <= 0 or content width <= 0 => no exclusion at all.
        assert_eq!(layout_initial_letter(0.0, 3, 1000.0, 20.0), (0.0, 0.0));
        assert_eq!(layout_initial_letter(-1.0, 3, 1000.0, 20.0), (0.0, 0.0));
        assert_eq!(layout_initial_letter(3.0, 3, 1000.0, 0.0), (0.0, 0.0));
        assert_eq!(layout_initial_letter(3.0, 3, 1000.0, -20.0), (0.0, 0.0));
        assert_eq!(layout_initial_letter(3.0, 3, 0.0, 20.0), (0.0, 0.0));
        assert_eq!(layout_initial_letter(3.0, 3, -10.0, 20.0), (0.0, 0.0));
    }

    #[test]
    fn layout_initial_letter_computes_a_classic_drop_cap() {
        // 3 lines x 20px => 60px tall; 60 * 0.7 + 4px gap => 46px wide.
        assert_eq!(layout_initial_letter(3.0, 3, 1000.0, 20.0), (46.0, 60.0));
        // The width is clamped to the content box.
        assert_eq!(layout_initial_letter(3.0, 3, 10.0, 20.0), (10.0, 60.0));
        // sink == 0 (raised cap): the exclusion still covers the whole letter.
        assert_eq!(layout_initial_letter(3.0, 0, 1000.0, 20.0), (46.0, 60.0));
        // sink > size (sunken cap): the exclusion grows past the letter.
        assert_eq!(layout_initial_letter(2.0, 5, 1000.0, 20.0), (32.0, 100.0));
    }

    #[test]
    fn layout_initial_letter_exclusion_is_never_shorter_than_the_letter() {
        for size_lines in [0.5_f32, 1.0, 3.0, 100.0] {
            for sink in [0_u32, 1, 3, 100] {
                let (w, h) = layout_initial_letter(size_lines, sink, 10_000.0, 20.0);
                let letter_height = size_lines * 20.0;
                assert!(
                    h >= letter_height,
                    "{size_lines}/{sink}: {h} < {letter_height}"
                );
                assert!(w > 0.0 && w.is_finite());
            }
        }
    }

    #[test]
    fn layout_initial_letter_saturates_at_extreme_sinks_and_sizes() {
        // u32::MAX lines of sink: a huge but finite exclusion, no overflow panic.
        let (w, h) = layout_initial_letter(1.0, u32::MAX, 1000.0, 20.0);
        assert!(h.is_finite() && h > 0.0);
        assert_eq!(w, 18.0); // 20 * 0.7 + 4

        // f32::MAX size: the height saturates to +inf, the width clamps to the box.
        let (w, h) = layout_initial_letter(f32::MAX, 1, 1000.0, f32::MAX);
        assert_eq!(w, 1000.0);
        assert!(h.is_infinite());
    }

    #[test]
    fn layout_initial_letter_with_nan_and_inf_produces_a_defined_result() {
        // NaN passes the `<= 0.0` guard (NaN compares false), so the maths runs.
        // f32::min/max discard NaN, so the result stays defined: the width falls back
        // to the content box and the height to the sink-derived exclusion.
        let (w, h) = layout_initial_letter(f32::NAN, 3, 1000.0, 20.0);
        assert_eq!(w, 1000.0);
        assert_eq!(h, 60.0);

        // An infinite line height clamps the width and yields an infinite exclusion.
        let (w, h) = layout_initial_letter(3.0, 3, 1000.0, f32::INFINITY);
        assert_eq!(w, 1000.0);
        assert!(h.is_infinite());
    }

    // ==================================================================
    // get_cell_spans (other)
    // ==================================================================

    fn cell_dom(attrs: Vec<AttributeType>) -> StyledDom {
        let mut cell = Dom::create_div();
        for a in attrs {
            cell = cell.with_attribute(a);
        }
        styled(Dom::create_body().with_child(cell), "")
    }

    #[test]
    fn get_cell_spans_defaults_to_one_when_unset() {
        let dom = cell_dom(Vec::new());
        assert_eq!(get_cell_spans(&dom, DIV_NODE), (1, 1));
    }

    #[test]
    fn get_cell_spans_clamps_hostile_html_values() {
        // Zero / negative / i32::MIN spans must clamp up to 1, never wrap or panic.
        for n in [0, -1, i32::MIN] {
            let dom = cell_dom(vec![AttributeType::ColSpan(n), AttributeType::RowSpan(n)]);
            assert_eq!(get_cell_spans(&dom, DIV_NODE), (1, 1), "span {n}");
        }
        // Absurd spans clamp down to the HTML limits (1000 / 65534) so the column
        // and row vectors cannot be grown into an OOM.
        let dom = cell_dom(vec![
            AttributeType::ColSpan(i32::MAX),
            AttributeType::RowSpan(i32::MAX),
        ]);
        assert_eq!(get_cell_spans(&dom, DIV_NODE), (1000, 65534));

        // In-range values pass through.
        let dom = cell_dom(vec![AttributeType::ColSpan(3), AttributeType::RowSpan(7)]);
        assert_eq!(get_cell_spans(&dom, DIV_NODE), (3, 7));
    }

    // ==================================================================
    // get_float_property / get_clear_property (other)
    // ==================================================================

    #[test]
    fn get_float_and_clear_properties_default_to_none_for_a_missing_node() {
        let dom = text_dom("x", "");
        assert_eq!(get_float_property(&dom, None), LayoutFloat::None);
        assert_eq!(get_clear_property(&dom, None), LayoutClear::None);
        // An unstyled node also reports the initial values.
        assert_eq!(get_float_property(&dom, Some(DIV_NODE)), LayoutFloat::None);
        assert_eq!(get_clear_property(&dom, Some(DIV_NODE)), LayoutClear::None);
    }

    #[test]
    fn get_float_and_clear_properties_read_the_cascade() {
        let dom = text_dom("x", ".p { float: right; clear: both; }");
        assert_eq!(get_float_property(&dom, Some(DIV_NODE)), LayoutFloat::Right);
        assert_eq!(get_clear_property(&dom, Some(DIV_NODE)), LayoutClear::Both);

        let dom = text_dom("x", ".p { float: left; clear: left; }");
        assert_eq!(get_float_property(&dom, Some(DIV_NODE)), LayoutFloat::Left);
        assert_eq!(get_clear_property(&dom, Some(DIV_NODE)), LayoutClear::Left);
    }

    // ==================================================================
    // split_text_for_whitespace (other)
    // ==================================================================

    #[test]
    fn split_text_for_whitespace_collapses_runs_in_normal_mode() {
        let dom = text_dom("  a  b  ", "");
        let out = split_text_for_whitespace(&dom, TEXT_NODE, "  a  b  ", &plain_style());
        assert_eq!(out.len(), 1);
        // Interior runs collapse to one space; the leading/trailing ones are kept
        // as a single space each (they are trimmed later, at line-layout time).
        assert_eq!(text_of(&out[0]), Some(" a b "));
    }

    #[test]
    fn split_text_for_whitespace_on_empty_and_whitespace_only_text() {
        let dom = text_dom("", "");
        assert!(split_text_for_whitespace(&dom, TEXT_NODE, "", &plain_style()).is_empty());

        // A whitespace-only node collapses to exactly one space.
        let out = split_text_for_whitespace(&dom, TEXT_NODE, "   \t  ", &plain_style());
        assert_eq!(out.len(), 1);
        assert_eq!(text_of(&out[0]), Some(" "));
    }

    #[test]
    fn split_text_for_whitespace_honours_newlines_in_pre() {
        let dom = text_dom("a\nb", ".p { white-space: pre; }");
        let out = split_text_for_whitespace(&dom, TEXT_NODE, "a\nb", &plain_style());
        assert_eq!(out.len(), 3);
        assert_eq!(text_of(&out[0]), Some("a"));
        assert!(matches!(out[1], InlineContent::LineBreak(_)));
        assert_eq!(text_of(&out[2]), Some("b"));

        // Tabs become explicit Tab items so the tab-size can be applied later.
        let out = split_text_for_whitespace(&dom, TEXT_NODE, "a\tb", &plain_style());
        assert_eq!(out.len(), 3);
        assert_eq!(text_of(&out[0]), Some("a"));
        assert!(matches!(out[1], InlineContent::Tab { .. }));
        assert_eq!(text_of(&out[2]), Some("b"));

        // Whitespace is preserved verbatim in `pre`.
        let out = split_text_for_whitespace(&dom, TEXT_NODE, "  a  ", &plain_style());
        assert_eq!(text_of(&out[0]), Some("  a  "));
    }

    #[test]
    fn split_text_for_whitespace_normalises_cr_and_crlf() {
        let dom = text_dom("x", ".p { white-space: pre; }");
        // \r\n and a bare \r must behave exactly like \n (one forced break each).
        for text in ["a\r\nb", "a\rb", "a\nb"] {
            let out = split_text_for_whitespace(&dom, TEXT_NODE, text, &plain_style());
            assert_eq!(out.len(), 3, "{text:?}");
            assert!(matches!(out[1], InlineContent::LineBreak(_)), "{text:?}");
        }
    }

    #[test]
    fn split_text_for_whitespace_forces_breaks_on_bk_nl_chars_in_every_mode() {
        // U+2028 LINE SEPARATOR is a forced break even in white-space: normal.
        let dom = text_dom("x", "");
        let out = split_text_for_whitespace(&dom, TEXT_NODE, "a\u{2028}b", &plain_style());
        assert_eq!(out.len(), 3);
        assert!(matches!(out[1], InlineContent::LineBreak(_)));

        let dom = text_dom("x", ".p { white-space: pre-line; }");
        let out = split_text_for_whitespace(&dom, TEXT_NODE, "a\u{000C}b", &plain_style());
        assert_eq!(out.len(), 3);
        assert!(matches!(out[1], InlineContent::LineBreak(_)));
    }

    #[test]
    fn split_text_for_whitespace_strips_bidi_controls_before_collapsing() {
        let dom = text_dom("x", "");
        // The RLM between the two spaces must not stop them from collapsing.
        let out = split_text_for_whitespace(&dom, TEXT_NODE, "a \u{200F} b", &plain_style());
        assert_eq!(out.len(), 1);
        assert_eq!(text_of(&out[0]), Some("a b"));
    }

    #[test]
    fn split_text_for_whitespace_pre_line_collapses_spaces_but_keeps_breaks() {
        let dom = text_dom("x", ".p { white-space: pre-line; }");
        let out = split_text_for_whitespace(&dom, TEXT_NODE, "a  b\n  c  ", &plain_style());
        assert_eq!(out.len(), 3);
        assert_eq!(text_of(&out[0]), Some("a b"));
        assert!(matches!(out[1], InlineContent::LineBreak(_)));
        assert_eq!(text_of(&out[2]), Some("c"));
    }

    #[test]
    fn split_text_for_whitespace_survives_a_huge_multibyte_input() {
        let dom = text_dom("x", ".p { white-space: pre; }");
        let text = "🌍\n".repeat(2_000);
        let out = split_text_for_whitespace(&dom, TEXT_NODE, &text, &plain_style());
        // 2000 globes + 2000 breaks (the trailing empty segment emits no Text item).
        assert_eq!(out.len(), 4_000);
        assert_eq!(text_of(&out[0]), Some("🌍"));
    }
