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

    #[test]
    fn a_padded_paragraph_in_a_flex_start_column_is_no_wider_than_the_column() {
        let mut dom = Dom::create_body()
            .with_css(
                "display: flex; flex-direction: column; align-items: flex-start; margin: 0px; \
                 width: 300px; font-size: 16px;",
            )
            .with_child(
                Dom::create_div()
                    .with_class("p".into())
                    .with_css("padding: 0px 20px; margin: 0px 10px; border: 2px solid black;")
                    .with_child(Dom::create_span_with_text(
                        "a paragraph long enough to wrap over several lines in the column it sits in",
                    )),
            );
        let styled = StyledDom::create(&mut dom, azul_css::css::Css::empty());
        let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
        let mut ws = FullWindowState::default();
        ws.size.dimensions = LogicalSize::new(800.0, 600.0);
        lw.current_window_state = ws.clone();
        lw.layout_and_generate_display_list(
            styled,
            &ws,
            &RendererResources::default(),
            &ExternalSystemCallbacks::rust_internal(),
            &mut None,
        )
        .expect("the page lays out");
        let lr = &lw.layout_results[&DomId::ROOT_ID];
        let p = lr
            .styled_dom
            .node_data
            .as_container()
            .internal
            .iter()
            .position(|n| n.has_class("p"))
            .expect("the paragraph");
        let index = *lr
            .layout_tree
            .dom_to_layout
            .get(&NodeId::new(p))
            .and_then(|v| v.first())
            .expect("laid out");
        let size = lr.layout_tree.get(index).and_then(|n| n.used_size).expect("sized");
        // 300 px column, 10 px margin each side: the border box gets at most 280.
        assert!(
            size.width <= 280.5,
            "the paragraph's border box is {} px wide in a 300 px column with 10 px margins",
            size.width
        );
    }
