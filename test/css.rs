
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
fn test_case_issue_93_2() {
    use azul_css::*;

    use self::CssPathSelector::*;

    let parsed_css = new_from_str(
        "
        .tabwidget-tab-label {
          color: #FFFFFF;
        }

        .tabwidget-tab.active .tabwidget-tab-label {
          color: #000000;
        }

        .tabwidget-tab.active .tabwidget-tab-close {
          color: #FF0000;
        }
    ",
    )
    .unwrap();

    fn declaration(classes: &[CssPathSelector], color: ColorU) -> CssRuleBlock {
        CssRuleBlock {
            path: CssPath {
                selectors: classes.to_vec().into(),
            },
            declarations: vec![CssDeclaration::Static(CssProperty::TextColor(
                CssPropertyValue::Exact(StyleTextColor { inner: color }),
            ))]
            .into(),
        }
    }

    let expected_rules = vec![
        declaration(
            &[Class("tabwidget-tab-label".to_string().into())],
            ColorU {
                r: 255,
                g: 255,
                b: 255,
                a: 255,
            },
        ),
        declaration(
            &[
                Class("tabwidget-tab".to_string().into()),
                Class("active".to_string().into()),
                Children,
                Class("tabwidget-tab-label".to_string().into()),
            ],
            ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
        ),
        declaration(
            &[
                Class("tabwidget-tab".to_string().into()),
                Class("active".to_string().into()),
                Children,
                Class("tabwidget-tab-close".to_string().into()),
            ],
            ColorU {
                r: 255,
                g: 0,
                b: 0,
                a: 255,
            },
        ),
    ];

    assert_eq!(
        parsed_css,
        Css {
            stylesheets: vec![expected_rules.into()].into()
        }
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

#[test]
fn test_css_pseudo_selector_parse() {
    use azul_css::{CssNthChildPattern, CssNthChildSelector::*};

    use self::{CssPathPseudoSelector::*, CssPseudoSelectorParseError::*};
    let ok_res = [
        (("first", None), First),
        (("last", None), Last),
        (("hover", None), Hover),
        (("active", None), Active),
        (("focus", None), Focus),
        (("nth-child", Some("4")), NthChild(Number(4))),
        (("nth-child", Some("even")), NthChild(Even)),
        (("nth-child", Some("odd")), NthChild(Odd)),
        (
            ("nth-child", Some("5n")),
            NthChild(Pattern(CssNthChildPattern {
                repeat: 5,
                offset: 0,
            })),
        ),
        (
            ("nth-child", Some("2n+3")),
            NthChild(Pattern(CssNthChildPattern {
                repeat: 2,
                offset: 3,
            })),
        ),
    ];

    let err = [
        (("asdf", None), UnknownSelector("asdf", None)),
        (("", None), UnknownSelector("", None)),
        (("nth-child", Some("2n+")), InvalidNthChildPattern("2n+")),
        // Can't test for ParseIntError because the fields are private.
        // This is an example on why you shouldn't use core::error::Error!
    ];

    for ((selector, val), a) in &ok_res {
        assert_eq!(pseudo_selector_from_str(selector, *val), Ok(*a));
    }

    for ((selector, val), e) in &err {
        assert_eq!(pseudo_selector_from_str(selector, *val), Err(e.clone()));
    }
}


#[test]
fn test_css_parse_1() {
    use azul_css::*;

    let parsed_css = new_from_str(
        "
        div#my_id .my_class:first {
            background-color: red;
        }
    ",
    )
    .unwrap();

    let expected_css_rules = vec![CssRuleBlock {
        path: CssPath {
            selectors: vec![
                CssPathSelector::Type(NodeTypeTag::Div),
                CssPathSelector::Id("my_id".to_string().into()),
                CssPathSelector::Children,
                // NOTE: This is technically wrong, the space between "#my_id"
                // and ".my_class" is important, but gets ignored for now
                CssPathSelector::Class("my_class".to_string().into()),
                CssPathSelector::PseudoSelector(CssPathPseudoSelector::First),
            ]
            .into(),
        },
        declarations: vec![CssDeclaration::Static(CssProperty::BackgroundContent(
            CssPropertyValue::Exact(
                vec![StyleBackgroundContent::Color(ColorU {
                    r: 255,
                    g: 0,
                    b: 0,
                    a: 255,
                })]
                .into(),
            ),
        ))]
        .into(),
    }]
    .into();

    assert_eq!(
        parsed_css,
        Css {
            stylesheets: vec![expected_css_rules].into(),
        }
    );
}

#[test]
fn test_css_simple_selector_parse() {
    use azul_css::NodeTypeTag;

    use self::CssPathSelector::*;
    let css = "div#id.my_class > p .new { }";
    let parsed = vec![
        Type(NodeTypeTag::Div),
        Id("id".to_string().into()),
        Class("my_class".to_string().into()),
        DirectChildren,
        Type(NodeTypeTag::P),
        Children,
        Class("new".to_string().into()),
    ];
    assert_eq!(
        new_from_str(css).unwrap(),
        Css {
            stylesheets: vec![Stylesheet {
                rules: vec![CssRuleBlock {
                    path: CssPath {
                        selectors: parsed.into()
                    },
                    declarations: Vec::new().into(),
                }]
                .into(),
            }]
            .into(),
        }
    );
}

fn test_css(css: &str, expected: Vec<CssRuleBlock>) {
    let css = new_from_str(css).unwrap();
    assert_eq!(
        css,
        Css {
            stylesheets: vec![expected.into()].into()
        }
    );
}

// Tests that an element with a single class always gets the CSS element applied properly
#[test]
fn test_apply_css_pure_class() {
    let red = CssProperty::BackgroundContent(CssPropertyValue::Exact(
        StyleBackgroundContentVec::from(vec![StyleBackgroundContent::Color(ColorU {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        })]),
    ));
    let blue = CssProperty::BackgroundContent(CssPropertyValue::Exact(
        StyleBackgroundContentVec::from(vec![StyleBackgroundContent::Color(ColorU {
            r: 0,
            g: 0,
            b: 255,
            a: 255,
        })]),
    ));
    let black = CssProperty::BackgroundContent(CssPropertyValue::Exact(
        StyleBackgroundContentVec::from(vec![StyleBackgroundContent::Color(ColorU {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        })]),
    ));

    // Simple example
    {
        let css_1 = ".my_class { background-color: red; }";
        let expected_rules = vec![CssRuleBlock {
            path: CssPath {
                selectors: vec![CssPathSelector::Class("my_class".to_string().into())].into(),
            },
            declarations: vec![CssDeclaration::Static(red.clone())].into(),
        }]
        .into();
        test_css(css_1, expected_rules);
    }

    // Slightly more complex example
    {
        let css_2 = "#my_id { background-color: red; } .my_class { background-color: blue; }";
        let expected_rules = vec![
            CssRuleBlock {
                path: CssPath {
                    selectors: vec![CssPathSelector::Id("my_id".to_string().into())].into(),
                },
                declarations: vec![CssDeclaration::Static(red.clone())].into(),
            },
            CssRuleBlock {
                path: CssPath {
                    selectors: vec![CssPathSelector::Class("my_class".to_string().into())]
                        .into(),
                },
                declarations: vec![CssDeclaration::Static(blue.clone())].into(),
            },
        ];
        test_css(css_2, expected_rules);
    }

    // Even more complex example
    {
        let css_3 = "* { background-color: black; } .my_class#my_id { background-color: red; \
                        } .my_class { background-color: blue; }";
        let expected_rules = vec![
            CssRuleBlock {
                path: CssPath {
                    selectors: vec![CssPathSelector::Global].into(),
                },
                declarations: vec![CssDeclaration::Static(black.clone())].into(),
            },
            CssRuleBlock {
                path: CssPath {
                    selectors: vec![
                        CssPathSelector::Class("my_class".to_string().into()),
                        CssPathSelector::Id("my_id".to_string().into()),
                    ]
                    .into(),
                },
                declarations: vec![CssDeclaration::Static(red.clone())].into(),
            },
            CssRuleBlock {
                path: CssPath {
                    selectors: vec![CssPathSelector::Class("my_class".to_string().into())]
                        .into(),
                },
                declarations: vec![CssDeclaration::Static(blue.clone())].into(),
            },
        ]
        .into();
        test_css(css_3, expected_rules);
    }
}


// Assert that order of the style rules is correct (in same order as provided in CSS form)
#[test]
fn test_multiple_rules() {
    use azul_css::*;

    use self::CssPathSelector::*;

    let parsed_css = new_from_str(
        "
        * { }
        * div.my_class#my_id { }
        * div#my_id { }
        * #my_id { }
        div.my_class.specific#my_id { }
    ",
    )
    .unwrap();

    let expected_rules = vec![
        // Rules are sorted by order of appearance in source string
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
                    Type(NodeTypeTag::Div),
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
                    Global,
                    Type(NodeTypeTag::Div),
                    Id("my_id".to_string().into()),
                ]
                .into(),
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
                    Type(NodeTypeTag::Div),
                    Class("my_class".to_string().into()),
                    Class("specific".to_string().into()),
                    Id("my_id".to_string().into()),
                ]
                .into(),
            },
            declarations: Vec::new().into(),
        },
    ];

    assert_eq!(
        parsed_css,
        Css {
            stylesheets: vec![expected_rules.into()].into()
        }
    );
}
