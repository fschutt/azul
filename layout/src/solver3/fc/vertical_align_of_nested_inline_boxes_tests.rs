    use azul_core::{
        dom::{Dom, DomId, NodeId},
        geom::LogicalSize,
        resources::RendererResources,
        styled_dom::StyledDom,
    };
    use rust_fontconfig::FcFontCache;

    use crate::{
        callbacks::ExternalSystemCallbacks, text3::cache::ShapedItem, window::LayoutWindow,
        window_state::FullWindowState,
    };

    fn text(s: &'static str) -> Dom {
        Dom::create_text_do_not_use_without_block_level_wrapper(s)
    }

    /// `body(0) > p(1) > "Cock"(2) sup(3)>"a"(4) " and Job"(5)
    /// span(6)>"b"(7) " and x"(8) sub(9)>"2"(10) " and n"(11)
    /// sup(12)>i(13)>"1"(14) " and "(15) sup(16)>["x"(17) sup(18)>"y"(19)]`.
    fn page() -> StyledDom {
        let mut dom = Dom::create_body().with_child(
            Dom::create_p()
                .with_child(text("Cock"))
                .with_child(Dom::create_sup_with_text("a"))
                .with_child(text(" and Job"))
                .with_child(
                    Dom::create_span_with_text("b")
                        .with_css("vertical-align: super; font-size: 0.6em;"),
                )
                .with_child(text(" and x"))
                .with_child(Dom::create_sub_with_text("2"))
                .with_child(text(" and n"))
                .with_child(Dom::create_sup().with_child(Dom::create_i().with_child(text("1"))))
                .with_child(text(" and "))
                .with_child(
                    Dom::create_sup()
                        .with_child(text("x"))
                        .with_child(Dom::create_sup_with_text("y")),
                ),
        );
        let (css, _) = azul_css::parser2::new_from_str(
            "body { margin: 0; font-size: 16px; } p { margin: 0; line-height: 40px; }",
        );
        StyledDom::create(&mut dom, css)
    }

    /// `(baseline y, font size)` of the first cluster of text node `node` in
    /// the paragraph's line layout.
    fn baseline_of(lw: &LayoutWindow, node: usize) -> (f32, f32) {
        let lr = &lw.layout_results[&DomId::ROOT_ID];
        let p = *lr
            .layout_tree
            .dom_to_layout
            .get(&NodeId::new(1))
            .and_then(|v| v.first())
            .expect("the paragraph is laid out");
        let layout = lr
            .layout_tree
            .materialized_inline_layout_for_node(p.index())
            .expect("the paragraph holds lines");
        layout
            .items
            .iter()
            .find_map(|it| match &it.item {
                ShapedItem::Cluster(c) if c.source_node_id == Some(NodeId::new(node)) => {
                    let (ascent, _) =
                        crate::text3::cache::get_item_vertical_metrics_approx(&it.item);
                    Some((it.position.y + ascent, c.style.font_size_px))
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("text node {node} is on the line"))
    }

    #[test]
    fn sup_sub_and_vertical_align_super_shift_their_text_and_a_nested_box_rides_the_shift() {
        let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
        let mut ws = FullWindowState::default();
        ws.size.dimensions = LogicalSize::new(800.0, 600.0);
        lw.current_window_state = ws.clone();
        lw.layout_and_generate_display_list(
            page(),
            &ws,
            &RendererResources::default(),
            &ExternalSystemCallbacks::rust_internal(),
            &mut None,
        )
        .expect("the page lays out");

        let (base, _) = baseline_of(&lw, 2);
        let up = 16.0 / 3.0 + 1.0;
        let down = 16.0 / 5.0 + 1.0;
        let near = |a: f32, b: f32| (a - b).abs() < 0.5;
        for (node, what, expected) in [
            (4, "<sup>a</sup>", base - up),
            (7, "a span with vertical-align: super", base - up),
            (10, "<sub>2</sub>", base + down),
            (14, "the <i> inside a <sup> (a footnote mark)", base - up),
            (17, "the outer <sup>'s own text", base - up),
        ] {
            let (y, _) = baseline_of(&lw, node);
            assert!(
                near(y, expected),
                "{what}: its baseline is {y}, the line's {base}, expected {expected} (16px / 3 \
                 + 1 up, / 5 + 1 down)"
            );
        }
        // A superscript of a superscript: the outer one's shift plus its own,
        // measured against the OUTER sup's font size (its parent).
        let (_, outer_fs) = baseline_of(&lw, 17);
        let (y, _) = baseline_of(&lw, 19);
        let expected = base - up - (outer_fs / 3.0 + 1.0);
        assert!(
            near(y, expected),
            "a <sup> in a <sup> rises by both shifts: its baseline is {y}, expected {expected} \
             (the line's {base}, the outer sup's font {outer_fs}px)"
        );
    }
