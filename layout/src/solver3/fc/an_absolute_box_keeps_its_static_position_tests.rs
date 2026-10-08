    use azul_core::{
        dom::{Dom, DomId, NodeId},
        geom::LogicalSize,
        resources::RendererResources,
        styled_dom::StyledDom,
    };
    use rust_fontconfig::FcFontCache;

    use crate::{
        callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
    };

    /// `<p>text</p><div abs 100x100></div><div next 100x100></div>`.
    fn page(text: &str) -> Vec<Dom> {
        alloc::vec![
            Dom::create_p()
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(text)),
            Dom::create_div()
                .with_class("abs".into())
                .with_css("position: absolute; width: 100px; height: 100px;"),
            Dom::create_div()
                .with_class("next".into())
                .with_css("width: 100px; height: 100px;"),
        ]
    }

    fn lay_out(lw: &mut LayoutWindow, mut dom: Dom) {
        let styled = StyledDom::create(&mut dom, azul_css::css::Css::empty());
        let mut ws = FullWindowState::default();
        ws.size.dimensions = LogicalSize::new(800.0, 600.0);
        lw.current_window_state = ws.clone();
        lw.layout_new_generation(
            styled,
            &ws,
            &RendererResources::default(),
            &ExternalSystemCallbacks::rust_internal(),
            &mut None,
        )
        .expect("the page lays out");
    }

    /// (the absolute box's y, the next block's y).
    fn ys(lw: &LayoutWindow) -> (f32, f32) {
        let lr = &lw.layout_results[&DomId::ROOT_ID];
        let y_of = |class: &str| {
            let dom = lr
                .styled_dom
                .node_data
                .as_container()
                .internal
                .iter()
                .position(|n| n.has_class(class))
                .expect("the node");
            let index = *lr
                .layout_tree
                .dom_to_layout
                .get(&NodeId::new(dom))
                .and_then(|v| v.first())
                .expect("laid out");
            crate::solver3::pos_get(&lr.calculated_positions, index.index())
                .expect("positioned")
                .y
        };
        (y_of("abs"), y_of("next"))
    }

    /// The body as the root: its first paragraph's top margin cannot escape,
    /// the root moved its in-flow children down by it - and left the
    /// absolute box's static position that margin above the next block.
    #[test]
    fn an_absolute_box_after_a_paragraph_starts_where_the_next_block_does() {
        let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
        lay_out(&mut lw, Dom::create_body().with_children(page("A paragraph.").into()));
        let (abs, next) = ys(&lw);
        assert!((abs - next).abs() < 0.5, "the absolute box starts at y {abs}, the block after it at y {next}");
    }

    /// WPT css/CSS2/tables/height-table-cell-001: `<html><body><p>..</p><div
    /// abs></div><table>`. The body is laid out at a provisional origin and
    /// moved after; the move re-derives its children from their
    /// `relative_position`, which the absolute box did not have: it went
    /// back to the body's content-box origin, over the paragraph.
    #[test]
    fn in_a_body_under_html_the_absolute_box_starts_where_the_next_block_does() {
        let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
        lay_out(
            &mut lw,
            Dom::create_html().with_child(Dom::create_body().with_children(page("A paragraph.").into())),
        );
        let (abs, next) = ys(&lw);
        assert!((abs - next).abs() < 0.5, "the absolute box starts at y {abs}, the block after it at y {next}");
    }

    /// A relayout after the paragraph grew a line: the absolute box moves
    /// down with the block after it (its position from the first layout is
    /// stale).
    #[test]
    fn after_the_paragraph_grows_the_absolute_box_moves_down_with_the_next_block() {
        let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
        lay_out(&mut lw, Dom::create_html().with_child(Dom::create_body().with_children(page("A paragraph.").into())));
        let (_, before) = ys(&lw);
        let long = "A paragraph long enough to wrap onto a second line in an eight hundred pixel \
                    wide window, and then onto a third one, with some more words to be sure of it.";
        lay_out(&mut lw, Dom::create_html().with_child(Dom::create_body().with_children(page(long).into())));
        let (abs, next) = ys(&lw);
        assert!(next > before + 1.0, "premise: the next block moved down ({before} -> {next})");
        assert!((abs - next).abs() < 0.5, "the absolute box starts at y {abs}, the block after it at y {next}");
    }

    /// A relayout that only MOVES the block holding the page (a block above
    /// it grew): the block's own layout is a cache hit, which re-placed its
    /// in-flow children and left the absolute box where the first layout
    /// had put it.
    #[test]
    fn when_its_block_moves_down_the_absolute_box_moves_with_it() {
        let doc = |above: u32| {
            Dom::create_html().with_child(
                Dom::create_body()
                    .with_child(Dom::create_div().with_css(&alloc::format!("height: {above}px;")))
                    .with_child(Dom::create_div().with_children(page("A paragraph.").into())),
            )
        };
        let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
        lay_out(&mut lw, doc(10));
        let (_, before) = ys(&lw);
        lay_out(&mut lw, doc(60));
        let (abs, next) = ys(&lw);

        assert!((next - before - 50.0).abs() < 0.5, "premise: the next block moved down 50 px ({before} -> {next})");
        assert!((abs - next).abs() < 0.5, "the absolute box starts at y {abs}, the block after it at y {next}");
    }
