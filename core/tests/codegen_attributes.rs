//! The CODE GENERATION half of the one attribute table
//! (`azul_core::xml::attributes`): an attribute that sets something on a node
//! becomes the builder call that sets the same thing, in api.json vocabulary
//! (`.with_tab_index(..)`, `.with_contenteditable(..)`,
//! `.with_attribute(AttributeType::..)`, the `direction` in `with_css`). A
//! value code cannot express yet says why in the item's doc. A round trip
//! checks that the parser and the generated code set the same thing. Needs
//! azul-core's `codegen` feature.

use azul_core::{
    codegen::{backend, dom::lower_xml_fragment},
    dom::{AttributeNameValue, AttributeType, Dom, TabIndex},
    window::{AzStringPair, StringPairVec},
    xml::{
        attributes::{register_xml_attribute, AttributeScope, NodeSetting, XmlAttribute},
        str_to_dom_unstyled, ComponentMap, XmlAttributeMap, XmlNode, XmlNodeChild,
    },
};
use azul_css::{
    codegen::ir::{EnumShape, Expr, Item, Prim},
    AzString,
};

fn node(tag: &str, attrs: &[(&str, &str)], children: Vec<XmlNodeChild>) -> XmlNode {
    XmlNode {
        node_type: tag.into(),
        attributes: XmlAttributeMap::from(StringPairVec::from_vec(
            attrs
                .iter()
                .map(|(k, v)| AzStringPair {
                    key: AzString::from(*k),
                    value: AzString::from(*v),
                })
                .collect::<Vec<_>>(),
        )),
        children: children.into(),
    }
}

fn el(tag: &str, attrs: &[(&str, &str)], children: Vec<XmlNodeChild>) -> XmlNodeChild {
    XmlNodeChild::Element(node(tag, attrs, children))
}

fn txt(s: &str) -> XmlNodeChild {
    XmlNodeChild::Text(AzString::from(s))
}

fn dom(m: &str, args: Vec<Expr>) -> Expr {
    Expr::call("Dom", m, args)
}

fn with(recv: Expr, m: &str, args: Vec<Expr>) -> Expr {
    Expr::method(recv, "Dom", m, args)
}

fn attr(variant: &str, args: Vec<Expr>) -> Expr {
    Expr::variant("AttributeType", EnumShape::Tagged, variant, args)
}

fn lowered(nodes: Vec<XmlNodeChild>) -> Item {
    lower_xml_fragment(&nodes, "", "render_x", None, Vec::new())
        .items
        .remove(0)
}

#[test]
fn tabindex_and_focusable_become_with_tab_index() {
    assert_eq!(
        lowered(vec![el("div", &[("tabindex", "3")], vec![])]).value,
        with(
            dom("create_div", vec![]),
            "with_tab_index",
            vec![Expr::variant(
                "TabIndex",
                EnumShape::Tagged,
                "OverrideInParent",
                vec![Expr::int(3, Prim::U32)]
            )]
        )
    );
    assert_eq!(
        lowered(vec![el("div", &[("focusable", "true")], vec![])]).value,
        with(
            dom("create_div", vec![]),
            "with_tab_index",
            vec![Expr::unit("TabIndex", EnumShape::Tagged, "Auto")]
        )
    );
}

#[test]
fn contenteditable_becomes_with_contenteditable_or_the_explicit_false_attribute() {
    assert_eq!(
        lowered(vec![el("div", &[("contenteditable", "true")], vec![])]).value,
        with(dom("create_div", vec![]), "with_contenteditable", vec![Expr::Bool(true)])
    );
    assert_eq!(
        lowered(vec![el("div", &[("contenteditable", "false")], vec![])]).value,
        with(
            dom("create_div", vec![]),
            "with_attribute",
            vec![attr("ContentEditable", vec![Expr::Bool(false)])]
        )
    );
}

#[test]
fn typed_attributes_become_with_attribute() {
    assert_eq!(
        lowered(vec![el(
            "td",
            &[("autofocus", ""), ("placeholder", "Name"), ("colspan", "2")],
            vec![txt("x")]
        )])
        .value,
        with(
            with(
                with(
                    dom("create_td_with_text", vec![Expr::str("x")]),
                    "with_attribute",
                    vec![Expr::unit("AttributeType", EnumShape::Tagged, "Autofocus")]
                ),
                "with_attribute",
                vec![attr("Placeholder", vec![Expr::str("Name")])]
            ),
            "with_attribute",
            vec![attr("ColSpan", vec![Expr::int(2, Prim::I32)])]
        )
    );
}

#[test]
fn a_form_controls_attributes_follow_its_constructor_without_repeating_what_it_took() {
    // `type` and `name` are the constructor's arguments; the rest are
    // attributes, `data-azul-*` switches included.
    let x = lowered(vec![el(
        "input",
        &[
            ("type", "range"),
            ("name", "v"),
            ("min", "0"),
            ("required", ""),
            ("data-azul-widget", "none"),
        ],
        vec![],
    )]);
    let ctor = dom(
        "create_input_no_a11y",
        vec![Expr::str("range"), Expr::str("v"), Expr::str("")],
    );
    let want = with(
        with(
            with(ctor, "with_attribute", vec![attr("Min", vec![Expr::str("0")])]),
            "with_attribute",
            vec![Expr::unit("AttributeType", EnumShape::Tagged, "Required")],
        ),
        "with_attribute",
        vec![attr(
            "Data",
            vec![Expr::strukt(
                "AttributeNameValue",
                vec![
                    ("attr_name", Expr::str("data-azul-widget")),
                    ("value", Expr::str("none")),
                ],
            )],
        )],
    );
    assert_eq!(x.value, want);
}

#[test]
fn dir_joins_the_inline_css_before_the_style_attribute() {
    assert_eq!(
        lowered(vec![el("div", &[("style", "color: red"), ("dir", "rtl")], vec![])]).value,
        with(
            dom("create_div", vec![]),
            "with_css",
            vec![Expr::str("direction: rtl; color: red")]
        )
    );
}

#[test]
fn a_value_code_cannot_express_says_why_in_the_items_doc() {
    let x = lowered(vec![el(
        "p",
        &[("data-l10n", "greeting"), ("onclick", "save")],
        vec![txt("Hi")],
    )]);
    assert_eq!(x.value, dom("create_p_with_text", vec![Expr::str("Hi")]));
    assert!(x.doc.iter().any(|d| d.contains("data-l10n")), "{:?}", x.doc);
    assert!(
        x.doc.iter().any(|d| d.contains("onclick") && d.contains("callback")),
        "{:?}",
        x.doc
    );
}

#[test]
fn an_attribute_an_app_registers_is_generated_as_what_it_sets() {
    fn hint(_name: &str, value: &str) -> Option<NodeSetting> {
        Some(NodeSetting::Attribute(AttributeType::Custom(AttributeNameValue {
            attr_name: "hint".into(),
            value: value.into(),
        })))
    }
    register_xml_attribute(XmlAttribute {
        name: "x-b6-codegen-hint",
        scope: AttributeScope::AnyElement,
        order: 50,
        setting: hint,
    });
    assert_eq!(
        lowered(vec![el("div", &[("x-b6-codegen-hint", "hello")], vec![])]).value,
        with(
            dom("create_div", vec![]),
            "with_attribute",
            vec![attr(
                "Custom",
                vec![Expr::strukt(
                    "AttributeNameValue",
                    vec![("attr_name", Expr::str("hint")), ("value", Expr::str("hello"))],
                )]
            )]
        )
    );
}

#[test]
fn markup_and_the_generated_rust_set_the_same_node() {
    let attrs = [("tabindex", "3"), ("contenteditable", "false"), ("placeholder", "Name")];
    // markup -> Dom
    let body = node("body", &[], vec![el("div", &attrs, vec![])]);
    let map = ComponentMap::with_builtin();
    let parsed: Dom = str_to_dom_unstyled(&[XmlNodeChild::Element(body)], &map).expect("parses");
    let nd = parsed.children.as_ref()[0].children.as_ref()[0].root.clone();
    assert_eq!(nd.get_tab_index(), Some(TabIndex::OverrideInParent(3)));
    let a = nd.attributes().as_ref().to_vec();
    assert!(a.contains(&AttributeType::ContentEditable(false)), "{a:?}");
    assert!(a.contains(&AttributeType::Placeholder("Name".into())), "{a:?}");
    // markup -> Rust: the builder calls that set exactly these.
    let m = lower_xml_fragment(&[el("div", &attrs, vec![])], "", "render_x", None, Vec::new());
    let rust = backend("rust").expect("rust").emit_module(&m);
    for want in [
        ".with_tab_index(TabIndex::OverrideInParent(3))",
        ".with_attribute(AttributeType::ContentEditable(false))",
        ".with_attribute(AttributeType::Placeholder(azul::str::String::from(\"Name\")))",
    ] {
        assert!(rust.contains(want), "expected `{want}` in:\n{rust}");
    }
}
