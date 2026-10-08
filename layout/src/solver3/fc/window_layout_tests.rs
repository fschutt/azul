    use azul_core::{
        dom::{Dom, DomId, IdOrClass, NodeId},
        geom::{LogicalPosition, LogicalSize},
        resources::RendererResources,
        styled_dom::StyledDom,
        task::Instant,
    };
    use rust_fontconfig::FcFontCache;

    use crate::{
        callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
    };

    fn styled(mut dom: Dom, css: &str) -> StyledDom {
        let (css, _) = azul_css::parser2::new_from_str(css);
        StyledDom::create(&mut dom, css)
    }

    fn lay_out(lw: &mut LayoutWindow, styled_dom: StyledDom) {
        let mut ws = FullWindowState::default();
        ws.size.dimensions = LogicalSize::new(800.0, 600.0);
        lw.current_window_state = ws.clone();
        lw.layout_and_generate_display_list(
            styled_dom,
            &ws,
            &RendererResources::default(),
            &ExternalSystemCallbacks::rust_internal(),
            &mut None,
        )
        .expect("the page lays out");
    }

    /// The first page, then the same DOM under `second_css` - rebuilt as the
    /// shell rebuilds (the CSS diff first) in the same window.
    fn restyled(lw: &mut LayoutWindow, page: fn() -> Dom, first_css: &str, second_css: &str) {
        lay_out(lw, styled(page(), first_css));
        let mut next = styled(page(), second_css);
        let _pending = lw.begin_reconciliation(DomId::ROOT_ID, &mut next, Instant::now());
        lay_out(lw, next);
    }

    /// The used size of DOM node `node`.
    fn size_of(lw: &LayoutWindow, node: usize) -> LogicalSize {
        let lr = &lw.layout_results[&DomId::ROOT_ID];
        let index = *lr
            .layout_tree
            .dom_to_layout
            .get(&NodeId::new(node))
            .and_then(|v| v.first())
            .expect("the node is laid out");
        lr.layout_tree
            .get(index)
            .and_then(|n| n.used_size)
            .expect("the node has a size")
    }

    /// The calculated position of DOM node `node`.
    fn position_of(lw: &LayoutWindow, node: usize) -> LogicalPosition {
        let lr = &lw.layout_results[&DomId::ROOT_ID];
        let index = *lr
            .layout_tree
            .dom_to_layout
            .get(&NodeId::new(node))
            .and_then(|v| v.first())
            .expect("the node is laid out");
        *lr.calculated_positions
            .get(index.index())
            .expect("the node has a position")
    }

    /// `body(0) > div.p(1) > "Hello Hello Hello"(2)`.
    fn paragraph() -> Dom {
        Dom::create_body().with_child(
            Dom::create_div()
                .with_ids_and_classes(vec![IdOrClass::Class("p".into())].into())
                .with_child(
                    Dom::create_span_with_text("Hello Hello Hello")
                        .with_ids_and_classes(vec![IdOrClass::Class("s".into())].into()),
                ),
        )
    }

    #[test]
    fn a_stylesheet_only_font_size_change_relays_out_its_text() {
        // `body(0) > div.p(1) > span.s(2) > text(3)`. A fixed 20px line box,
        // so the height counts the lines whatever the font: three words of
        // 10px text fit one 200px line (about 80px), at 40px each word is
        // about 100px wide and they wrap to 2 or 3 lines. The span's padding
        // moves too: a font-size change alone is classified paint-only by
        // `begin_reconciliation` (`relayout_scope(false)`, a round-2 note),
        // the padding makes the span's restyle a layout one - lifted to the
        // paragraph, whose cached collection kept the 10px runs.
        let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
        let base = "body { margin: 0; } .p { width: 200px; line-height: 20px; }";
        restyled(
            &mut lw,
            paragraph,
            &format!("{base} .s {{ font-size: 10px; padding-left: 0px; }}"),
            &format!("{base} .s {{ font-size: 40px; padding-left: 1px; }}"),
        );
        let h = size_of(&lw, 1).height;
        assert!(
            h >= 39.5,
            "40px words wrap the 200px paragraph to two lines or more (it kept the 10px runs \
             of the cached collection: one line): {h}"
        );
    }

    /// `body(0) > div.p(1) > [i.a(2)][i.b(3)]`, two inline-blocks on one line.
    fn two_inline_blocks() -> Dom {
        let ib = |class: &'static str| {
            Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class(class.into())].into())
        };
        Dom::create_body().with_child(
            Dom::create_div()
                .with_ids_and_classes(vec![IdOrClass::Class("p".into())].into())
                .with_child(ib("a"))
                .with_child(ib("b")),
        )
    }

    #[test]
    fn a_stylesheet_only_width_change_of_an_inline_block_moves_what_follows_it() {
        // Font-free: the collection caches each atomic inline's measured
        // size (`InlineShape`), so a stale key keeps the old 30px slot.
        let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
        let base = "body { margin: 0; } .a, .b { display: inline-block; height: 10px; } .b { \
                    width: 10px; }";
        restyled(
            &mut lw,
            two_inline_blocks,
            &format!("{base} .a {{ width: 30px; }}"),
            &format!("{base} .a {{ width: 60px; }}"),
        );
        let a = size_of(&lw, 2).width;
        assert!(
            (a - 60.0).abs() < 0.5,
            "the first inline-block is 60 wide now: {a}"
        );
        let b = position_of(&lw, 3).x;
        assert!(
            (b - 60.0).abs() < 0.5,
            "the second inline-block starts after the 60px one: {b}"
        );
    }

    /// `body(0) > div.ul(1) > div.li(2) > div.block(3)`: a list item whose
    /// first child is a block with no line box in it.
    fn a_list_item_whose_first_child_is_a_block() -> Dom {
        let class = |c: &'static str| -> azul_core::dom::IdOrClassVec {
            vec![IdOrClass::Class(c.into())].into()
        };
        Dom::create_body().with_child(
            Dom::create_div()
                .with_ids_and_classes(class("ul"))
                .with_child(
                    Dom::create_div()
                        .with_ids_and_classes(class("li"))
                        .with_child(Dom::create_div().with_ids_and_classes(class("block"))),
                ),
        )
    }

    #[test]
    fn a_list_item_without_a_line_box_is_as_tall_as_its_block() {
        // `<li><div style="height: 50px"></div></li>`: no line box for the
        // marker to ride (`marker_line_host` is None). Chrome places the
        // marker at the item's content start, out of the flow, and the item
        // is as tall as the taller of the two (LayoutNG's
        // PositionListMarkerWithoutLineBoxes): 50. The marker took a 20px
        // line of its own above the block: 70.
        let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
        lay_out(
            &mut lw,
            styled(
                a_list_item_whose_first_child_is_a_block(),
                "body { margin: 0; } .ul { margin: 0; padding-left: 40px; } .li { display: \
                 list-item; list-style-type: disc; line-height: 20px; } .block { height: 50px; }",
            ),
        );
        let li = size_of(&lw, 2).height;
        assert!(
            (li - 50.0).abs() < 0.5,
            "the item is its block's 50px: {li}"
        );
        let item_top = position_of(&lw, 2).y;
        let block_top = position_of(&lw, 3).y;
        assert!(
            (block_top - item_top).abs() < 0.5,
            "the block starts at the item's top, no marker line above it: {block_top} vs \
             {item_top}"
        );
    }

    /// `body(0) > div.root(1) > [div.mover(2), div.after(3)]`.
    fn a_block_above_a_block() -> Dom {
        let div = |c: &'static str| {
            Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class(c.into())].into())
        };
        Dom::create_body().with_child(
            div("root")
                .with_child(div("mover"))
                .with_child(div("after")),
        )
    }

    #[test]
    fn a_block_moves_with_its_own_margin_after_a_restyle() {
        // LAYOUTPERF8 bug B (scripts/layoutperf8_e2e/a_block_moves_with_its_own_margin.json):
        // a stylesheet change of a block's own margin-left (0 -> 40px) moves
        // it to x = 40, where a fresh layout of the page puts it. It stayed
        // at 0: the node was a clean CLONE carrying the box props of the OLD
        // cascade, the css-dirty channel only marked it dirty, and a dirty
        // block root is re-solved in its old slot.
        let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
        let base = "body { margin: 0; } .mover { width: 50px; height: 20px; } .after { height: \
                    20px; }";
        restyled(
            &mut lw,
            a_block_above_a_block,
            &format!("{base} .mover {{ margin-left: 0px; }}"),
            &format!("{base} .mover {{ margin-left: 40px; }}"),
        );
        let mover = position_of(&lw, 2);
        assert!(
            (mover.x - 40.0).abs() < 0.5,
            "the block moved with its margin: {mover:?}"
        );
        let after = position_of(&lw, 3);
        assert!(
            after.x.abs() < 0.5 && (after.y - 20.0).abs() < 0.5,
            "the block below stays in its slot: {after:?}"
        );
    }

    #[test]
    fn an_empty_inline_with_padding_is_as_tall_as_its_strut() {
        // `body(0) > div.p(1) > span.e(2)`, the span empty and padded. CSS
        // 2.2 10.8.1: an inline box with no glyphs holds a strut - it is
        // line-height tall and straddles the baseline like the line's own
        // strut; its vertical padding and borders do not count in the line
        // box. Chrome: 18. It was a line-height + padding tall box sitting
        // ON the baseline: 18 + 8 above the baseline plus the strut's
        // descent below it (the brief's 27.2 with `line-height: normal`).
        // Font-free: both struts take the 0.8em / 0.2em fallback split.
        let page = || {
            Dom::create_body().with_child(
                Dom::create_div()
                    .with_ids_and_classes(vec![IdOrClass::Class("p".into())].into())
                    .with_child(
                        Dom::create_span()
                            .with_ids_and_classes(vec![IdOrClass::Class("e".into())].into()),
                    ),
            )
        };
        let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
        lay_out(
            &mut lw,
            styled(
                page(),
                "body { margin: 0; } .p { font-size: 16px; line-height: 18px; } .e { padding: \
                 4px; }",
            ),
        );
        let h = size_of(&lw, 1).height;
        assert!((h - 18.0).abs() < 0.5, "the line is its strut's 18px: {h}");
    }

    /// `body(0) > .column(1) > .split(2) > .half(3) > .pane(4) > .row(5)`:
    /// the chain of every OfficeShell app (AzNews, AzCode) - a `flex-grow`
    /// column, a split pane, its `display: block` half and the `height: 100%`
    /// pane in it.
    fn a_percentage_height_pane_in_a_split_half() -> Dom {
        let div = |c: &'static str| {
            Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class(c.into())].into())
        };
        Dom::create_body().with_child(
            div("column").with_child(
                div("split").with_child(div("half").with_child(div("pane").with_child(div("row")))),
            ),
        )
    }

    #[test]
    fn a_percentage_height_measured_against_an_indefinite_height_is_auto() {
        // R2-APPS: AzNews' and AzCode's E2E screenshots were blank, stderr
        // reported a compositor layer 22,598 px / infinitely tall, and the
        // shell's whole chain (`get_all_nodes_layout`) had an infinite height.
        // Measuring the half's content height (a flex basis, a row's cross
        // size) lays it out under an INDEFINITE height (`INFINITY` in
        // `available_size`), and the pane's `height: 100%` was multiplied by
        // it. CSS 2.2 10.5: a percentage of an indefinite height computes to
        // `auto` - the measure is the pane's content (50px). The final layout
        // then gives the stretched half its definite 600px, and the pane's
        // 100% resolves against that. Chrome: all four boxes 600px tall.
        let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
        lay_out(
            &mut lw,
            styled(
                a_percentage_height_pane_in_a_split_half(),
                "body { margin: 0; display: flex; flex-direction: column; height: 600px; } \
                 .column { display: flex; flex-direction: column; flex-grow: 1; min-height: \
                 0px; } .split { display: flex; flex-direction: row; width: 100%; height: \
                 100%; flex-grow: 1; overflow: hidden; } .half { display: block; flex-grow: 1; \
                 flex-basis: 0px; min-width: 0px; min-height: 0px; overflow: hidden; } .pane { \
                 display: flex; flex-direction: column; width: 100%; height: 100%; } .row { \
                 height: 50px; flex-shrink: 0; }",
            ),
        );
        for (node, name) in [(1, "column"), (2, "split"), (3, "half"), (4, "pane")] {
            let h = size_of(&lw, node).height;
            assert!(
                h.is_finite() && (h - 600.0).abs() < 0.5,
                "the {name} fills the 600px body (Chrome 600): {h}"
            );
        }
    }

    /// `body(0) > [div.above(1), div.abs(2), div.below(3)]`.
    fn an_absolute_box_between_two_blocks() -> Dom {
        let div = |c: &'static str| {
            Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class(c.into())].into())
        };
        Dom::create_body()
            .with_child(div("above"))
            .with_child(div("abs"))
            .with_child(div("below"))
    }

    #[test]
    fn an_absolute_box_with_auto_insets_sits_where_the_flow_would_have_put_it() {
        // WPT css/CSS2/tables/height-table-cell-001 (and every "no red"
        // test with an `overlapped-red-reference` after its `<p>`): an
        // absolutely positioned box whose insets are all `auto` takes its
        // STATIC position - where it would have been as a block in the flow
        // (CSS 2.2 10.3.7 / 10.6.4): after the block above it and that
        // block's bottom margin, at its own margin-left. Chrome: (5, 40),
        // and the block below starts at 40 too (the absolute box takes no
        // room). It sat at its parent's content-box origin, (0, 0): on top
        // of the paragraph, the red reference showing above the green.
        let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
        lay_out(
            &mut lw,
            styled(
                an_absolute_box_between_two_blocks(),
                "body { margin: 0; } .above { height: 30px; margin-bottom: 10px; } .abs { \
                 position: absolute; width: 10px; height: 10px; margin-left: 5px; } .below { \
                 height: 20px; }",
            ),
        );
        let abs = position_of(&lw, 2);
        assert!(
            (abs.y - 40.0).abs() < 0.5 && (abs.x - 5.0).abs() < 0.5,
            "the absolute box sits below the 30px block and its 10px margin, at its own 5px \
             margin (Chrome (5, 40)): {abs:?}"
        );
        let below = position_of(&lw, 3);
        assert!(
            (below.y - 40.0).abs() < 0.5,
            "the absolute box takes no room in the flow: the block below starts at 40: {below:?}"
        );
    }

    /// `body(0) > [div.li(1) > div.block(2)], [div.li(3) > div.p(4) >
    /// span(5) > "Text"(6)]`: two list items whose first child is a block.
    fn two_list_items_starting_with_a_block() -> Dom {
        let class = |c: &'static str| -> azul_core::dom::IdOrClassVec {
            vec![IdOrClass::Class(c.into())].into()
        };
        Dom::create_body()
            .with_child(
                Dom::create_div()
                    .with_ids_and_classes(class("li"))
                    .with_child(Dom::create_div().with_ids_and_classes(class("block"))),
            )
            .with_child(
                Dom::create_div().with_ids_and_classes(class("li")).with_child(
                    Dom::create_div()
                        .with_ids_and_classes(class("p"))
                        .with_child(Dom::create_span_with_text("Text")),
                ),
            )
    }

    #[test]
    fn an_inside_marker_is_a_line_of_its_own_before_a_block_child() {
        // WPT css/CSS2/lists/list-style-position-023: an INSIDE marker is an
        // inline box at the start of its item (CSS 2.2 12.5.1). Before a
        // block child it sits in an anonymous block of its own - one line -
        // and the block starts below it; it never rides a nested block's
        // first line. Chrome: each item is 20px (the marker's line) taller
        // than its block, the block 20px down. The item's first line box
        // was looked for down its first blocks (`marker_line_host`, the
        // OUTSIDE marker's rule): the marker of the item with a text block
        // rode that text's line (three nested `<li>`s piled "1. 1. 1." onto
        // the innermost line), the one of the item with an empty block hung
        // out of the flow at its top.
        let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
        lay_out(
            &mut lw,
            styled(
                two_list_items_starting_with_a_block(),
                "body { margin: 0; } .li { display: list-item; list-style-type: disc; \
                 list-style-position: inside; line-height: 20px; margin: 0; padding: 0; } \
                 .block { height: 50px; } .p { margin: 0; }",
            ),
        );
        for (item, block, block_height, what) in [
            (1, 2, 50.0, "an empty 50px block"),
            (3, 4, 20.0, "a block of one 20px line"),
        ] {
            let h = size_of(&lw, item).height;
            assert!(
                (h - (block_height + 20.0)).abs() < 0.5,
                "the item holding {what} is the marker's 20px line taller (Chrome {}): {h}",
                block_height + 20.0
            );
            let down = position_of(&lw, block).y - position_of(&lw, item).y;
            assert!(
                (down - 20.0).abs() < 0.5,
                "{what} starts below the marker's line, 20px into its item: {down}"
            );
        }
    }

    /// Where the first marker glyph of list item `item` (a DOM node) sits,
    /// relative to the content box of the IFC holding it: the item's own
    /// line layout, else its marker box's (which sits at the item's content
    /// start).
    fn marker_x(lw: &LayoutWindow, item: usize) -> f32 {
        let tree = &lw.layout_results[&DomId::ROOT_ID].layout_tree;
        let li = tree
            .dom_to_layout
            .get(&NodeId::new(item))
            .and_then(|v| v.first())
            .expect("the item is laid out")
            .index();
        std::iter::once(li)
            .chain(tree.children(li).iter().copied())
            .filter_map(|host| tree.materialized_inline_layout_for_node(host))
            .find_map(|layout| {
                layout.items.iter().find_map(|it| match &it.item {
                    crate::text3::cache::ShapedItem::Cluster(c)
                        if c.marker_position_outside.is_some() =>
                    {
                        Some(it.position.x)
                    }
                    _ => None,
                })
            })
            .unwrap_or_else(|| panic!("list item {item} has a marker"))
    }

    #[test]
    fn an_empty_list_items_marker_hangs_where_a_full_ones_does() {
        // WPT css/CSS2/lists/list-style-type-applies-to-009: the square of
        // an EMPTY list item was 4px (a space) closer to the content than
        // the square of an item with text. The marker text ends in a space
        // ("\u{25AA} "); alone on its line - an empty item's marker is a
        // line of its own - that space was the line's trailing white space
        // and was stripped, so the marker, placed by its width, moved in by
        // it. A marker's space is part of the marker (Chrome's UA sheet:
        // `::marker { white-space: pre }`) whatever follows it.
        // `body(0) > [div.li(1) > span(2) > "Item"(3)], [div.li(4)]`.
        let page = || {
            let li = || {
                Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class("li".into())].into())
            };
            Dom::create_body()
                .with_child(li().with_child(Dom::create_span_with_text("Item")))
                .with_child(li())
        };
        let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
        lay_out(
            &mut lw,
            styled(
                page(),
                "body { margin: 0; } .li { display: list-item; list-style-type: square; \
                 margin-left: 96px; line-height: 20px; }",
            ),
        );
        let full = marker_x(&lw, 1);
        let empty = marker_x(&lw, 4);
        assert!(
            full < 0.0,
            "harness: an outside marker hangs before its item's content: {full}"
        );
        assert!(
            (full - empty).abs() < 0.01,
            "the empty item's marker hangs where the full item's does: {empty} vs {full}"
        );
    }

    #[test]
    fn a_middle_aligned_cell_of_one_line_puts_its_line_at_its_top() {
        // WPT html/rendering/non-replaced-elements/tables/table-cell-nowrap-
        // with-fixed-width: a cell holding one line of a 100px inline-block.
        // The line box is the inline-block on the baseline plus the strut's
        // descent below it (20px text: 16 + 4, font-free), and the cell -
        // sized by that line box - is exactly as tall: `vertical-align:
        // middle` has nothing to centre. Chrome: the inline-block at the
        // cell's top. The alignment measured the content by its ITEMS'
        // bounds (the inline-block's 100px, not the line's 104px) and moved
        // it down by half the strut's descent (the green square 2px low).
        // `body(0) > div.t(1) > div.r(2) > div.c(3) > div.ib(4)`.
        let page = || {
            let div = |c: &'static str| {
                Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class(c.into())].into())
            };
            Dom::create_body()
                .with_child(div("t").with_child(div("r").with_child(div("c").with_child(div("ib")))))
        };
        let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
        lay_out(
            &mut lw,
            styled(
                page(),
                "body { margin: 0; font-size: 20px; line-height: 20px; } .t { display: table; \
                 border-spacing: 0; } .r { display: table-row; } .c { display: table-cell; \
                 vertical-align: middle; padding: 0; } .ib { display: inline-block; width: 50px; \
                 height: 100px; }",
            ),
        );
        let cell = size_of(&lw, 3).height;
        assert!(
            cell > 100.5,
            "harness: the cell holds the inline-block and the strut's descent: {cell}"
        );
        let down = position_of(&lw, 4).y - position_of(&lw, 3).y;
        assert!(
            down.abs() < 0.5,
            "the inline-block's line starts at the top of the cell it fills: {down}"
        );
    }

    #[test]
    fn a_border_width_in_inches_counts_in_a_collapsed_table_and_is_painted() {
        // WPT css/CSS2/tables/collapsing-border-model-003 / -009: a cell's
        // `border-top: 1in solid` is ONE collapsed edge of 96px, half in the
        // table's border, half in the cell's (CSS 2.2 17.6.2): an empty cell
        // makes the table 48 + 48 = 96px tall, the cell 48px down. The
        // compact cache stores only px widths - a `1in` (or `0.25em`) width
        // is a sentinel meaning "ask the cascade" - and the collapsed-border
        // resolution read the sentinel as 0: no edge, a 0px table. The
        // painter read it as no width (`medium`, 3px): a `0.5in` border
        // drew 3px wide around a box laid out 48px wide.
        // `body(0) > [div.t(1) > div.r(2) > div.c(3)], div.b(4)`.
        let page = || {
            let div = |c: &'static str| {
                Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class(c.into())].into())
            };
            Dom::create_body()
                .with_child(div("t").with_child(div("r").with_child(div("c"))))
                .with_child(div("b"))
        };
        let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
        lay_out(
            &mut lw,
            styled(
                page(),
                "body { margin: 0; } .t { display: table; border-collapse: collapse; } .r { \
                 display: table-row; } .c { display: table-cell; border-top: 1in solid orange; \
                 padding: 0; width: 50px; } .b { border-top: 0.5in solid red; width: 10px; \
                 height: 10px; }",
            ),
        );
        let table = size_of(&lw, 1).height;
        assert!(
            (table - 96.0).abs() < 0.5,
            "the 1in edge: 48px of table border + 48px of cell (Chrome 96): {table}"
        );
        let down = position_of(&lw, 3).y - position_of(&lw, 1).y;
        assert!(
            (down - 48.0).abs() < 0.5,
            "the cell starts below the table's half of the edge: {down}"
        );
        let painted = lw.layout_results[&DomId::ROOT_ID]
            .display_list
            .items
            .iter()
            .find_map(|item| match item {
                crate::solver3::display_list::DisplayListItem::Border { widths, styles, .. }
                    if styles
                        .top
                        .as_ref()
                        .and_then(|s| s.get_property())
                        .is_some_and(|s| {
                            s.inner == azul_css::props::style::border::BorderStyle::Solid
                        }) =>
                {
                    widths.top.as_ref().and_then(|w| w.get_property()).map(|w| w.inner)
                }
                _ => None,
            });
        assert_eq!(
            painted,
            Some(azul_css::props::basic::pixel::PixelValue::inch(0.5)),
            "the 0.5in border is painted 0.5in wide, not `medium`"
        );
    }

    #[test]
    fn a_column_with_a_definite_width_counts_without_cells() {
        // WPT css/css-tables/col-definite-size-001: four `<col style="width:
        // 100px">` over a row of two cells make a table of four 100px
        // columns (Chrome: as wide as the reference's four cells); a
        // trailing column a `<col>` alone makes with a percentage, a calc()
        // or no width names no column. The grid had a column only where a
        // cell was: the definite columns were dropped (200px).
        // `body(0) > div.t(1) > [div.g(2) > div.col(3..=6)], [div.r(7) > div.c(8), div.c(9)]`.
        let page = || {
            let div = |c: &'static str| {
                Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class(c.into())].into())
            };
            Dom::create_body().with_child(
                div("t")
                    .with_child(
                        div("g")
                            .with_child(div("col"))
                            .with_child(div("col"))
                            .with_child(div("col"))
                            .with_child(div("col")),
                    )
                    .with_child(div("r").with_child(div("c")).with_child(div("c"))),
            )
        };
        let table_width = |col_width: &str| {
            let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
            lay_out(
                &mut lw,
                styled(
                    page(),
                    &format!(
                        "body {{ margin: 0; }} .t {{ display: table; border-spacing: 0; }} .g \
                         {{ display: table-column-group; }} .col {{ display: table-column; \
                         width: {col_width}; }} .r {{ display: table-row; }} .c {{ display: \
                         table-cell; padding: 0; }}"
                    ),
                ),
            );
            size_of(&lw, 1).width
        };
        let definite = table_width("100px");
        assert!(
            (definite - 400.0).abs() < 0.5,
            "four 100px columns, two of them without cells (Chrome 400): {definite}"
        );
        // (The percentage / calc() / auto halves of the WPT page match
        // their reference already: those columns stay dropped.)
    }

    #[test]
    fn a_shrink_wrapped_list_item_keeps_its_text_on_its_markers_line() {
        // WPT css/css-lists/inline-block-list's reference: a list item as
        // wide as its text (`width: fit-content`, or shrink-to-fit in an
        // inline-block) showed its marker on one line and its text on the
        // next. An OUTSIDE marker hangs in the gutter - the line placement
        // never advances the pen for it, and the item's intrinsic width
        // leaves it out - but the line breaker counted its width on the
        // line, so the text no longer fitted beside it. Chrome: one 20px
        // line. `body(0) > div.ib(1) > div.li(2) > span(3) > "B"(4)`.
        let page = || {
            let div = |c: &'static str| {
                Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class(c.into())].into())
            };
            Dom::create_body().with_child(
                div("ib").with_child(div("li").with_child(Dom::create_span_with_text("B"))),
            )
        };
        let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
        lay_out(
            &mut lw,
            styled(
                page(),
                "body { margin: 0; padding-left: 40px; } .ib { display: inline-block; } .li { \
                 display: list-item; list-style-type: decimal; line-height: 20px; }",
            ),
        );
        let h = size_of(&lw, 2).height;
        assert!(
            (h - 20.0).abs() < 0.5,
            "the item's text stays on its marker's line (Chrome 20): {h}"
        );
    }
