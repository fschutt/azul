    use azul_core::{
        dom::{Dom, DomId, NodeId},
        geom::LogicalSize,
        resources::RendererResources,
        styled_dom::StyledDom,
    };
    use rust_fontconfig::FcFontCache;

    use crate::{
        callbacks::ExternalSystemCallbacks, widgets::text_input::TextInput, window::LayoutWindow,
        window_state::FullWindowState,
    };

    fn laid_out(mut dom: Dom) -> LayoutWindow {
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
        lw
    }

    /// The used size of the first node carrying the class `class`.
    fn size_of_class(lw: &LayoutWindow, class: &str) -> LogicalSize {
        let lr = &lw.layout_results[&DomId::ROOT_ID];
        let node = lr
            .styled_dom
            .node_data
            .as_container()
            .internal
            .iter()
            .position(|n| n.has_class(class))
            .unwrap_or_else(|| panic!("no node .{class}"));
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

    /// `body > row(flex, padding 6px 8px) > [block(flex-grow: 1) > search field, 177px box]`:
    /// AzContacts' list header.
    #[test]
    fn a_search_field_in_a_flex_grow_block_fills_it() {
        let page = Dom::create_body()
            .with_css("display: flex; flex-direction: column; margin: 0px; width: 360px;")
            .with_child(
                Dom::create_div()
                    .with_css(
                        "display: flex; flex-direction: row; align-items: center; padding: 6px \
                         8px;",
                    )
                    .with_child(
                        Dom::create_div()
                            .with_css("flex-grow: 1; margin-right: 6px;")
                            .with_child(
                                TextInput::create_search()
                                    .with_placeholder("Search contacts".into())
                                    .dom(),
                            ),
                    )
                    .with_child(
                        Dom::create_div().with_css("width: 177px; height: 30px; flex-shrink: 0;"),
                    ),
            );
        let lw = laid_out(page);
        let field = size_of_class(&lw, "__azul-native-search-field");
        let input = size_of_class(&lw, "__azul-native-text-input-container");
        // 360 - 2 x 8 padding - 177 - 6 margin = 161.
        assert!(
            field.width > 150.0,
            "the search field is {} px wide in a 161 px block",
            field.width
        );
        assert!(
            (input.width - field.width).abs() < 1.0,
            "the search field's input is {} px wide in a {} px field: its flex-grow left it at \
             its border and padding",
            input.width,
            field.width
        );
    }

    /// `body > row(flex) > [96px label, value(flex-grow: 1) > text]`: AzContacts' card rows.
    #[test]
    fn a_flex_grow_values_text_wraps_at_the_values_width() {
        let page = Dom::create_body()
            .with_css(
                "display: flex; flex-direction: column; margin: 0px; width: 400px; font-size: \
                 13px; line-height: 16px;",
            )
            .with_child(
                Dom::create_div()
                    .with_css(
                        "display: flex; flex-direction: row; align-items: flex-start; padding: \
                         3px 0px;",
                    )
                    .with_child(
                        Dom::create_div()
                            .with_css("width: 96px; flex-shrink: 0;")
                            .with_child(Dom::create_span_with_text("notes")),
                    )
                    .with_child(
                        Dom::create_div()
                            .with_class("value".into())
                            .with_css("flex-grow: 1;")
                            .with_child(Dom::create_span_with_text(
                                "A note long enough to be folded",
                            )),
                    ),
            );
        let lw = laid_out(page);
        let value = size_of_class(&lw, "value");
        assert!(
            value.width > 290.0,
            "the value is {} px wide next to a 96 px label in 400 px",
            value.width
        );
        // Seven words of 13 px text fit one 304 px line (about 200 px).
        assert!(
            value.height < 20.0,
            "the value's text takes {} px - {} line(s) of 16 px in a {} px value: it wrapped at \
             its longest word",
            value.height,
            (value.height / 16.0).round(),
            value.width
        );
    }

    fn flex_row(css: &str, children: alloc::vec::Vec<Dom>) -> Dom {
        Dom::create_div()
            .with_css(&alloc::format!("display: flex; flex-direction: row; align-items: center; {css}"))
            .with_children(children.into())
    }

    fn column(css: &str, children: alloc::vec::Vec<Dom>) -> Dom {
        Dom::create_div()
            .with_css(&alloc::format!("display: flex; flex-direction: column; {css}"))
            .with_children(children.into())
    }

    /// AzContacts' window as its `layout()` builds it (the E2E sweep of 2026-10-06, after the
    /// two tests above went green): the list header and a card row inside the real chrome - the
    /// theme scope's body, the PimShell's split panes, whose halves are `display: block` flex
    /// items with a `flex-grow` share, so every width under them is a flex item's FINAL size.
    fn contacts_window() -> Dom {
        use azul_css::{AzString, StringVec};

        use crate::widgets::{
            segmented::Segmented,
            shells::{PimShell, ShellThemeAccent, ShellThemeScope},
        };

        let search = TextInput::create_search()
            .with_placeholder("Search contacts".into())
            .dom();
        let sort = Segmented::create(StringVec::from_vec(alloc::vec![
            AzString::from("First name"),
            AzString::from("Last name"),
        ]))
        .dom();
        let list = column(
            "flex-grow: 1; min-height: 0px;",
            alloc::vec![
                flex_row(
                    "padding: 6px 8px;",
                    alloc::vec![
                        Dom::create_div()
                            .with_css("flex-grow: 1; margin-right: 6px;")
                            .with_child(search),
                        sort,
                    ],
                ),
                Dom::create_div()
                    .with_css("padding: 6px 8px 2px 8px; font-size: 12px;")
                    .with_child(Dom::create_span_with_text("All contacts")),
            ],
        );
        let notes = flex_row(
            "align-items: flex-start; padding: 3px 0px;",
            alloc::vec![
                Dom::create_div()
                    .with_css("width: 96px; flex-shrink: 0; font-size: 12px;")
                    .with_child(Dom::create_span_with_text("notes")),
                Dom::create_div()
                    .with_class("value".into())
                    .with_css("flex-grow: 1; font-size: 13px; line-height: 16px;")
                    .with_child(Dom::create_span_with_text(
                        "A note long enough to be folded",
                    )),
            ],
        );
        let reading = column("padding: 16px; flex-grow: 1; min-height: 0px;", alloc::vec![notes]);
        let navigation = column("padding: 8px;", alloc::vec![Dom::create_div().with_css("height: 30px;")]);
        let shell = PimShell::create(navigation, list, reading)
            .office_shell()
            .with_title_row(Dom::create_div().with_css("height: 32px;"))
            .with_status_bar(Dom::create_div().with_css("height: 22px;"))
            .dom();
        ShellThemeScope::create(column("flex-grow: 1; min-height: 0px;", alloc::vec![shell]))
            .with_accent(ShellThemeAccent::Blue)
            .body()
    }

    fn assert_the_contacts_widths(lw: &LayoutWindow, pass: &str) {
        let field = size_of_class(lw, "__azul-native-search-field");
        let input = size_of_class(lw, "__azul-native-text-input-container");
        assert!(
            field.width > 100.0,
            "{pass}: the search field is {} px wide in the list pane's header",
            field.width
        );
        assert!(
            (input.width - field.width).abs() < 1.0,
            "{pass}: the search field's input is {} px wide in a {} px field",
            input.width,
            field.width
        );
        let value = size_of_class(lw, "value");
        assert!(
            value.width > 200.0 && value.height < 20.0,
            "{pass}: the card's value is {} x {} px: its text wrapped at its longest word",
            value.width,
            value.height
        );
    }

    /// The first layout and a relayout of the same window (what a restyle or a rebuild runs):
    /// the search field's input fills the field, the card's value text is one line.
    #[test]
    fn in_the_contacts_window_a_flex_grow_items_content_takes_its_final_width() {
        let size = LogicalSize::new(1100.0, 720.0);
        let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
        let mut ws = FullWindowState::default();
        ws.size.dimensions = size;
        lw.current_window_state = ws.clone();
        for pass in ["the first layout", "the relayout"] {
            let mut dom = contacts_window();
            let styled = StyledDom::create(&mut dom, azul_css::css::Css::empty());
            lw.layout_and_generate_display_list(
                styled,
                &ws,
                &RendererResources::default(),
                &ExternalSystemCallbacks::rust_internal(),
                &mut None,
            )
            .expect("the window lays out");
            assert_the_contacts_widths(&lw, pass);
        }
    }
