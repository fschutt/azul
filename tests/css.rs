
#[test]
fn test_specificity() {
    use self::CssPathSelector::*;
    use alloc::string::ToString;
    assert_eq!(
        get_specificity(&CssPath {
            selectors: vec![Id("hello".to_string().into())].into()
        }),
        (1, 0, 0, 1)
    );
    assert_eq!(
        get_specificity(&CssPath {
            selectors: vec![Class("hello".to_string().into())].into()
        }),
        (0, 1, 0, 1)
    );
    assert_eq!(
        get_specificity(&CssPath {
            selectors: vec![Type(NodeTypeTag::Div)].into()
        }),
        (0, 0, 1, 1)
    );
    assert_eq!(
        get_specificity(&CssPath {
            selectors: vec![Id("hello".to_string().into()), Type(NodeTypeTag::Div)].into()
        }),
        (1, 0, 1, 2)
    );
}

// Assert that order of the style items is correct
// (in order of CSS path specificity, lowest-to-highest)
#[test]
fn test_specificity_sort() {
    use self::CssPathSelector::*;
    use crate::NodeTypeTag::*;
    use alloc::string::ToString;

    let input_style = Stylesheet {
        rules: vec![
            // Rules are sorted from lowest-specificity to highest specificity
            CssRuleBlock {
                path: CssPath {
                    selectors: vec![Global].into(),
                },
                declarations: Vec::new().into(),
            },
            CssRuleBlock {
                path: CssPath {
                    selectors: vec![
                        Global,
                        Type(Div),
                        Class("my_class".to_string().into()),
                        Id("my_id".to_string().into()),
                    ]
                    .into(),
                },
                declarations: Vec::new().into(),
            },
            CssRuleBlock {
                path: CssPath {
                    selectors: vec![Global, Type(Div), Id("my_id".to_string().into())].into(),
                },
                declarations: Vec::new().into(),
            },
            CssRuleBlock {
                path: CssPath {
                    selectors: vec![Global, Id("my_id".to_string().into())].into(),
                },
                declarations: Vec::new().into(),
            },
            CssRuleBlock {
                path: CssPath {
                    selectors: vec![
                        Type(Div),
                        Class("my_class".to_string().into()),
                        Class("specific".to_string().into()),
                        Id("my_id".to_string().into()),
                    ]
                    .into(),
                },
                declarations: Vec::new().into(),
            },
        ]
        .into(),
    }
    .sort_by_specificity();

    let expected_style = Stylesheet {
        rules: vec![
            // Rules are sorted from lowest-specificity to highest specificity
            CssRuleBlock {
                path: CssPath {
                    selectors: vec![Global].into(),
                },
                declarations: Vec::new().into(),
            },
            CssRuleBlock {
                path: CssPath {
                    selectors: vec![Global, Id("my_id".to_string().into())].into(),
                },
                declarations: Vec::new().into(),
            },
            CssRuleBlock {
                path: CssPath {
                    selectors: vec![Global, Type(Div), Id("my_id".to_string().into())].into(),
                },
                declarations: Vec::new().into(),
            },
            CssRuleBlock {
                path: CssPath {
                    selectors: vec![
                        Global,
                        Type(Div),
                        Class("my_class".to_string().into()),
                        Id("my_id".to_string().into()),
                    ]
                    .into(),
                },
                declarations: Vec::new().into(),
            },
            CssRuleBlock {
                path: CssPath {
                    selectors: vec![
                        Type(Div),
                        Class("my_class".to_string().into()),
                        Class("specific".to_string().into()),
                        Id("my_id".to_string().into()),
                    ]
                    .into(),
                },
                declarations: Vec::new().into(),
            },
        ]
        .into(),
    };

    assert_eq!(input_style, expected_style);
}

#[test]
fn test_case_issue_93() {
    use crate::dom::*;
    use azul_css::CssPathSelector::*;
    use azul_css::*;

    fn render_tab() -> Dom {
        Dom::div()
            .with_class("tabwidget-tab")
            .with_child(Dom::label("").with_class("tabwidget-tab-label"))
            .with_child(Dom::label("").with_class("tabwidget-tab-close"))
    }

    let dom = Dom::div().with_id("editor-rooms").with_child(
        Dom::div()
            .with_class("tabwidget-bar")
            .with_child(render_tab().with_class("active"))
            .with_child(render_tab())
            .with_child(render_tab())
            .with_child(render_tab()),
    );

    let dom = convert_dom_into_compact_dom(dom);

    let tab_active_close = CssPath {
        selectors: vec![
            Class("tabwidget-tab".to_string().into()),
            Class("active".to_string().into()),
            Children,
            Class("tabwidget-tab-close".to_string().into()),
        ]
        .into(),
    };

    let node_hierarchy = &dom.node_hierarchy;
    let node_data = &dom.node_data;
    let nodes_sorted: Vec<_> = node_hierarchy.as_ref().get_parents_sorted_by_depth();
    let html_node_tree = construct_html_cascade_tree(
        &node_hierarchy.as_ref(),
        &nodes_sorted,
    );

    //  rules: [
    //    ".tabwidget-tab-label"                        : ColorU::BLACK,
    //    ".tabwidget-tab.active .tabwidget-tab-label"  : ColorU::WHITE,
    //    ".tabwidget-tab.active .tabwidget-tab-close"  : ColorU::RED,
    //  ]

    //  0: [div #editor-rooms ]
    //   |-- 1: [div  .tabwidget-bar]
    //   |    |-- 2: [div  .tabwidget-tab .active]
    //   |    |    |-- 3: [p  .tabwidget-tab-label]
    //   |    |    |-- 4: [p  .tabwidget-tab-close]
    //   |    |-- 5: [div  .tabwidget-tab]
    //   |    |    |-- 6: [p  .tabwidget-tab-label]
    //   |    |    |-- 7: [p  .tabwidget-tab-close]
    //   |    |-- 8: [div  .tabwidget-tab]
    //   |    |    |-- 9: [p  .tabwidget-tab-label]
    //   |    |    |-- 10: [p  .tabwidget-tab-close]
    //   |    |-- 11: [div  .tabwidget-tab]
    //   |    |    |-- 12: [p  .tabwidget-tab-label]
    //   |    |    |-- 13: [p  .tabwidget-tab-close]

    // Test 1:
    // ".tabwidget-tab.active .tabwidget-tab-label"
    // should not match
    // ".tabwidget-tab.active .tabwidget-tab-close"
    assert_eq!(
        matches_html_element(
            &tab_active_close,
            NodeId::new(3),
            &node_hierarchy.as_container(),
            &node_data.as_ref(),
            &html_node_tree.as_ref(),
            None,
        ),
        false
    );

    // Test 2:
    // ".tabwidget-tab.active .tabwidget-tab-close"
    // should match
    // ".tabwidget-tab.active .tabwidget-tab-close"
    assert_eq!(
        matches_html_element(
            &tab_active_close,
            NodeId::new(4),
            &node_hierarchy.as_container(),
            &node_data.as_ref(),
            &html_node_tree.as_ref(),
            None,
        ),
        true
    );
}

#[test]
fn test_css_group_iterator() {
    use self::CssPathSelector::*;
    use azul_css::*;

    // ".hello > #id_text.new_class div.content"
    // -> ["div.content", "#id_text.new_class", ".hello"]
    let selectors = vec![
        Class("hello".to_string().into()),
        DirectChildren,
        Id("id_test".to_string().into()),
        Class("new_class".to_string().into()),
        Children,
        Type(NodeTypeTag::Div),
        Class("content".to_string().into()),
    ];

    let mut it = CssGroupIterator::new(&selectors);

    assert_eq!(
        it.next(),
        Some((
            vec![
                &Type(NodeTypeTag::Div),
                &Class("content".to_string().into()),
            ],
            CssGroupSplitReason::Children
        ))
    );

    assert_eq!(
        it.next(),
        Some((
            vec![
                &Id("id_test".to_string().into()),
                &Class("new_class".to_string().into()),
            ],
            CssGroupSplitReason::DirectChildren
        ))
    );

    assert_eq!(
        it.next(),
        Some((
            vec![&Class("hello".into()),],
            CssGroupSplitReason::DirectChildren
        ))
    ); // technically not correct

    assert_eq!(it.next(), None);

    // Test single class
    let selectors_2 = vec![Class("content".to_string().into())];

    let mut it = CssGroupIterator::new(&selectors_2);

    assert_eq!(
        it.next(),
        Some((
            vec![&Class("content".to_string().into()),],
            CssGroupSplitReason::Children
        ))
    );

    assert_eq!(it.next(), None);
}
