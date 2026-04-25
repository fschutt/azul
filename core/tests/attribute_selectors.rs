//! End-to-end matching tests for CSS attribute selectors against DOM nodes.

extern crate alloc;

use azul_core::{
    dom::{AttributeNameValue, AttributeType, Dom, IdOrClass, NodeType},
    id::NodeId,
    style::{construct_html_cascade_tree, matches_html_element},
    styled_dom::{convert_dom_into_compact_dom, CompactDom, NodeHierarchyItem, NodeHierarchyItemVec},
};
use azul_css::{
    css::{
        AttributeMatchOp, CssAttributeSelector, CssPath, CssPathSelector, NodeTypeTag,
    },
    corety::OptionString,
};

fn attr_selector(name: &str, op: AttributeMatchOp, value: Option<&str>) -> CssPathSelector {
    CssPathSelector::Attribute(CssAttributeSelector {
        name: name.to_string().into(),
        op,
        value: match value {
            Some(v) => OptionString::Some(v.to_string().into()),
            None => OptionString::None,
        },
    })
}

fn matches(dom: &CompactDom, node_id: NodeId, path: CssPath) -> bool {
    let nodes_sorted: alloc::vec::Vec<_> =
        dom.node_hierarchy.as_ref().get_parents_sorted_by_depth();
    let html_node_tree = construct_html_cascade_tree(
        &dom.node_hierarchy.as_ref(),
        &nodes_sorted,
        &dom.node_data.as_ref(),
    );
    let node_hierarchy_items: NodeHierarchyItemVec = dom
        .node_hierarchy
        .as_ref()
        .internal
        .iter()
        .map(|n| (*n).into())
        .collect::<alloc::vec::Vec<NodeHierarchyItem>>()
        .into();
    matches_html_element(
        &path,
        node_id,
        &node_hierarchy_items.as_container(),
        &dom.node_data.as_ref(),
        &html_node_tree.as_ref(),
        None,
    )
}

#[test]
fn matches_exists_with_data_attr() {
    let dom = Dom::create_div().with_attribute(AttributeType::Data(AttributeNameValue {
        attr_name: "data-foo".to_string().into(),
        value: "bar".to_string().into(),
    }));
    let dom = convert_dom_into_compact_dom(dom);

    let path = CssPath {
        selectors: vec![
            CssPathSelector::Type(NodeTypeTag::Div),
            attr_selector("data-foo", AttributeMatchOp::Exists, None),
        ]
        .into(),
    };
    assert!(matches(&dom, NodeId::new(0), path));
}

#[test]
fn does_not_match_when_attribute_missing() {
    let dom = Dom::create_div();
    let dom = convert_dom_into_compact_dom(dom);

    let path = CssPath {
        selectors: vec![attr_selector("data-foo", AttributeMatchOp::Exists, None)].into(),
    };
    assert!(!matches(&dom, NodeId::new(0), path));
}

#[test]
fn matches_eq_on_input_type() {
    let dom = Dom::create_node(NodeType::Input)
        .with_attribute(AttributeType::InputType("text".to_string().into()));
    let dom = convert_dom_into_compact_dom(dom);

    let matches_text = CssPath {
        selectors: vec![attr_selector("type", AttributeMatchOp::Eq, Some("text"))].into(),
    };
    assert!(matches(&dom, NodeId::new(0), matches_text));

    let does_not_match = CssPath {
        selectors: vec![attr_selector("type", AttributeMatchOp::Eq, Some("password"))].into(),
    };
    assert!(!matches(&dom, NodeId::new(0), does_not_match));
}

#[test]
fn matches_includes_word_in_class_attribute() {
    // class="foo primary bar" — `[class~="primary"]` should match.
    let dom = Dom::create_div().with_ids_and_classes(
        vec![
            IdOrClass::Class("foo".into()),
            IdOrClass::Class("primary".into()),
            IdOrClass::Class("bar".into()),
        ]
        .into(),
    );
    let dom = convert_dom_into_compact_dom(dom);

    let path = CssPath {
        selectors: vec![attr_selector(
            "class",
            AttributeMatchOp::Includes,
            Some("primary"),
        )]
        .into(),
    };
    assert!(matches(&dom, NodeId::new(0), path));
}

#[test]
fn includes_does_not_match_substring_inside_word() {
    // class="primary-button" — `[class~="primary"]` should NOT match
    // because `~=` requires whitespace-separated whole words.
    let dom = Dom::create_div().with_ids_and_classes(
        vec![IdOrClass::Class("primary-button".into())].into(),
    );
    let dom = convert_dom_into_compact_dom(dom);

    let path = CssPath {
        selectors: vec![attr_selector(
            "class",
            AttributeMatchOp::Includes,
            Some("primary"),
        )]
        .into(),
    };
    assert!(!matches(&dom, NodeId::new(0), path));
}

#[test]
fn matches_dashmatch_on_lang() {
    // `[lang|="en"]` matches `lang="en"` and `lang="en-US"` but not `lang="enzo"`.
    let en = convert_dom_into_compact_dom(
        Dom::create_div().with_attribute(AttributeType::Lang("en".to_string().into())),
    );
    let en_us = convert_dom_into_compact_dom(
        Dom::create_div().with_attribute(AttributeType::Lang("en-US".to_string().into())),
    );
    let enzo = convert_dom_into_compact_dom(
        Dom::create_div().with_attribute(AttributeType::Lang("enzo".to_string().into())),
    );

    let mk = || CssPath {
        selectors: vec![attr_selector("lang", AttributeMatchOp::DashMatch, Some("en"))].into(),
    };

    assert!(matches(&en, NodeId::new(0), mk()));
    assert!(matches(&en_us, NodeId::new(0), mk()));
    assert!(!matches(&enzo, NodeId::new(0), mk()));
}

#[test]
fn matches_prefix_suffix_substring_on_href() {
    let dom = convert_dom_into_compact_dom(
        Dom::create_node(NodeType::A)
            .with_attribute(AttributeType::Href(
                "https://example.com/file.pdf".to_string().into(),
            )),
    );

    let prefix = CssPath {
        selectors: vec![attr_selector("href", AttributeMatchOp::Prefix, Some("https://"))].into(),
    };
    assert!(matches(&dom, NodeId::new(0), prefix));

    let suffix = CssPath {
        selectors: vec![attr_selector("href", AttributeMatchOp::Suffix, Some(".pdf"))].into(),
    };
    assert!(matches(&dom, NodeId::new(0), suffix));

    let substring = CssPath {
        selectors: vec![attr_selector("href", AttributeMatchOp::Substring, Some("example"))]
            .into(),
    };
    assert!(matches(&dom, NodeId::new(0), substring));

    let bad_prefix = CssPath {
        selectors: vec![attr_selector("href", AttributeMatchOp::Prefix, Some("ftp://"))].into(),
    };
    assert!(!matches(&dom, NodeId::new(0), bad_prefix));
}

#[test]
fn empty_target_value_never_matches_substring_ops() {
    // `[href^=""]` and similar are commonly defined to never match in CSS.
    let dom = convert_dom_into_compact_dom(
        Dom::create_node(NodeType::A)
            .with_attribute(AttributeType::Href("https://example.com".to_string().into())),
    );
    for op in [
        AttributeMatchOp::Prefix,
        AttributeMatchOp::Suffix,
        AttributeMatchOp::Substring,
    ] {
        let path = CssPath {
            selectors: vec![attr_selector("href", op, Some(""))].into(),
        };
        assert!(
            !matches(&dom, NodeId::new(0), path),
            "empty target should not match for op {:?}",
            op
        );
    }
}

#[test]
fn id_attribute_selector_uses_attributes_storage() {
    // `#foo` lives in the same `attributes` vec as `[id="foo"]`, so the
    // attribute matcher must agree with the id matcher.
    let dom = convert_dom_into_compact_dom(
        Dom::create_div().with_ids_and_classes(vec![IdOrClass::Id("foo".into())].into()),
    );

    let path = CssPath {
        selectors: vec![attr_selector("id", AttributeMatchOp::Eq, Some("foo"))].into(),
    };
    assert!(matches(&dom, NodeId::new(0), path));
}
