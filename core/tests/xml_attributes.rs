//! ONE table maps an XML attribute to what it sets on a node
//! (`azul_core::xml::attributes`). The XML → DOM builders apply every
//! attribute through it; the code generator writes the builder call from the
//! same entry (core/tests/codegen_attributes.rs), so the two cannot disagree.
//!
//! These are the PARSING half: markup → the node's `NodeData`, one test per
//! attribute family, and an attribute an app registers.

use azul_core::{
    dom::{AttributeNameValue, AttributeType, Dom, NodeData, TabIndex},
    window::{AzStringPair, StringPairVec},
    xml::{
        attributes::{
            builtin_attributes, register_xml_attribute, AttributeScope, NodeSetting, XmlAttribute,
        },
        str_to_dom_unstyled, ComponentMap, XmlAttributeMap, XmlNode, XmlNodeChild,
    },
};
use azul_css::AzString;

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

fn txt(s: &str) -> XmlNodeChild {
    XmlNodeChild::Text(AzString::from(s))
}

/// The `NodeData` the XML → DOM builder makes of `<body><{tag} ..></body>`.
fn parsed(tag: &str, attrs: &[(&str, &str)], children: Vec<XmlNodeChild>) -> NodeData {
    let body = node("body", &[], vec![XmlNodeChild::Element(node(tag, attrs, children))]);
    let map = ComponentMap::with_builtin();
    let dom: Dom = str_to_dom_unstyled(&[XmlNodeChild::Element(body)], &map).expect("parses");
    // html > body > the element
    let body = &dom.children.as_ref()[0];
    body.children.as_ref()[0].root.clone()
}

fn attrs_of(nd: &NodeData) -> Vec<AttributeType> {
    nd.attributes().as_ref().to_vec()
}

#[test]
fn every_attribute_the_builders_understand_is_one_entry_of_the_table() {
    let names: Vec<&str> = builtin_attributes().iter().map(|a| a.name).collect();
    for want in [
        "id", "class", "style", "tabindex", "focusable", "contenteditable", "autofocus",
        "placeholder", "colspan", "rowspan", "dir", "type", "name", "value", "min", "max",
        "step", "pattern", "autocomplete", "aria-label", "title", "alt", "src", "minlength",
        "maxlength", "required", "disabled", "readonly", "selected", "checked", "data-l10n*",
        "data-*", "on*",
    ] {
        assert!(names.contains(&want), "{want} is not in the table: {names:?}");
    }
}

#[test]
fn id_and_class_become_the_nodes_ids_and_classes() {
    let nd = parsed("div", &[("id", "main"), ("class", "a b")], vec![]);
    let got: Vec<String> = nd
        .get_ids_and_classes()
        .as_ref()
        .iter()
        .map(|x| format!("{x:?}"))
        .collect();
    assert_eq!(got.len(), 3, "{got:?}");
    assert!(got[0].contains("main"), "{got:?}");
}

#[test]
fn tabindex_and_focusable_set_the_keyboard_focus() {
    assert_eq!(
        parsed("div", &[("tabindex", "3")], vec![]).get_tab_index(),
        Some(TabIndex::OverrideInParent(3))
    );
    assert_eq!(
        parsed("div", &[("tabindex", "0")], vec![]).get_tab_index(),
        Some(TabIndex::Auto)
    );
    assert_eq!(
        parsed("div", &[("tabindex", "-1")], vec![]).get_tab_index(),
        Some(TabIndex::NoKeyboardFocus)
    );
    assert_eq!(
        parsed("div", &[("focusable", "false")], vec![]).get_tab_index(),
        Some(TabIndex::NoKeyboardFocus)
    );
    // A later tabindex wins over focusable, as before.
    assert_eq!(
        parsed("div", &[("tabindex", "2"), ("focusable", "false")], vec![]).get_tab_index(),
        Some(TabIndex::OverrideInParent(2))
    );
}

#[test]
fn contenteditable_true_makes_the_node_editable_and_false_walls_it_off() {
    assert!(parsed("div", &[("contenteditable", "true")], vec![]).is_contenteditable());
    // An explicit false is NOT "no attribute": it walls the subtree off inside
    // an editable host. The two XML loaders used to disagree here (this one
    // dropped it); the table has one answer.
    let off = parsed("div", &[("contenteditable", "false")], vec![]);
    assert!(!off.is_contenteditable());
    assert!(
        attrs_of(&off).contains(&AttributeType::ContentEditable(false)),
        "{:?}",
        attrs_of(&off)
    );
}

#[test]
fn autofocus_placeholder_and_cell_spans_are_typed_attributes() {
    let a = attrs_of(&parsed(
        "td",
        &[("autofocus", ""), ("placeholder", "Name"), ("colspan", "2"), ("rowspan", "3")],
        vec![txt("x")],
    ));
    assert!(a.contains(&AttributeType::Autofocus), "{a:?}");
    assert!(a.contains(&AttributeType::Placeholder("Name".into())), "{a:?}");
    assert!(a.contains(&AttributeType::ColSpan(2)), "{a:?}");
    assert!(a.contains(&AttributeType::RowSpan(3)), "{a:?}");
}

#[test]
fn a_form_controls_attributes_are_typed_and_data_attributes_ride_along() {
    let a = attrs_of(&parsed(
        "input",
        &[
            ("type", "range"),
            ("min", "0"),
            ("max", "10"),
            ("required", ""),
            ("disabled", "false"),
            ("data-azul-widget", "none"),
        ],
        vec![],
    ));
    assert!(a.contains(&AttributeType::InputType("range".into())), "{a:?}");
    assert!(a.contains(&AttributeType::Min("0".into())), "{a:?}");
    assert!(a.contains(&AttributeType::Max("10".into())), "{a:?}");
    assert!(a.contains(&AttributeType::Required), "{a:?}");
    assert!(!a.contains(&AttributeType::Disabled), "disabled=\"false\" is off: {a:?}");
    assert!(
        a.contains(&AttributeType::Data(AttributeNameValue {
            attr_name: "data-azul-widget".into(),
            value: "none".into(),
        })),
        "{a:?}"
    );
    // Only form controls take them.
    let div = attrs_of(&parsed("div", &[("min", "0"), ("data-x", "1")], vec![]));
    assert!(div.is_empty(), "{div:?}");
}

#[test]
fn dir_and_style_become_the_nodes_inline_css_style_last() {
    let nd = parsed("div", &[("style", "color: red"), ("dir", "rtl")], vec![]);
    let css = format!("{:?}", nd.style);
    let dir = css.find("Rtl").expect("the direction");
    let color = css.find("TextColor").expect("the style attribute");
    assert!(dir < color, "direction first, the author's style last: {css}");
}

#[test]
fn an_attribute_an_app_registers_is_applied_by_the_builders() {
    fn hint(_name: &str, value: &str) -> Option<NodeSetting> {
        Some(NodeSetting::Attribute(AttributeType::Custom(AttributeNameValue {
            attr_name: "hint".into(),
            value: value.into(),
        })))
    }
    register_xml_attribute(XmlAttribute {
        name: "x-b6-parse-hint",
        scope: AttributeScope::AnyElement,
        order: 50,
        setting: hint,
    });
    let a = attrs_of(&parsed("div", &[("x-b6-parse-hint", "hello")], vec![]));
    assert!(
        a.contains(&AttributeType::Custom(AttributeNameValue {
            attr_name: "hint".into(),
            value: "hello".into(),
        })),
        "{a:?}"
    );
}

// ---- presentational attributes (TABLE_A) ----
//
// HTML's rendering section maps the legacy attributes of table elements to
// CSS ("presentational hints"). The table KEEPS them on the node, verbatim
// (an app reads `<td width>` like any attribute), and the cascade maps them
// to declarations (`presentational_css`) that precede the element's own
// `style` attribute.

fn custom(name: &str, value: &str) -> AttributeType {
    AttributeType::Custom(AttributeNameValue {
        attr_name: name.into(),
        value: value.into(),
    })
}

#[test]
fn presentational_attributes_of_table_elements_stay_on_the_node() {
    let a = attrs_of(&parsed(
        "table",
        &[
            ("width", "600"),
            ("border", ""),
            ("cellpadding", "4"),
            ("bgcolor", "#fff"),
        ],
        vec![],
    ));
    for want in [
        custom("width", "600"),
        custom("border", ""),
        custom("cellpadding", "4"),
        custom("bgcolor", "#fff"),
    ] {
        assert!(a.contains(&want), "{want:?} kept on the table: {a:?}");
    }
    let td = attrs_of(&parsed(
        "td",
        &[("align", "center"), ("valign", "top"), ("nowrap", "nowrap")],
        vec![],
    ));
    for want in [
        custom("align", "center"),
        custom("valign", "top"),
        custom("nowrap", "nowrap"),
    ] {
        assert!(td.contains(&want), "{want:?} kept on the cell: {td:?}");
    }
    // Not a table element: `width` on a div is no presentational attribute.
    let div = attrs_of(&parsed("div", &[("width", "600")], vec![]));
    assert!(!div.contains(&custom("width", "600")), "{div:?}");
}

mod presentational_css {
    use azul_core::xml::attributes::presentational_css;

    fn has(css: &str, decl: &str) -> bool {
        css.split(';').any(|d| d.trim() == decl)
    }

    #[test]
    fn width_and_height_map_to_the_dimension_properties_ignoring_zero() {
        let css = presentational_css("table", &[("width", "600")], &[]);
        assert!(has(&css, "width: 600px"), "{css}");
        let css = presentational_css("td", &[("width", " 50% ")], &[]);
        assert!(has(&css, "width: 50%"), "{css}");
        let css = presentational_css("td", &[("width", "120px")], &[]);
        assert!(has(&css, "width: 120px"), "{css}");
        let css = presentational_css("table", &[("width", "0")], &[]);
        assert!(!css.contains("width"), "zero is ignored: {css}");
        let css = presentational_css("table", &[("width", "wide")], &[]);
        assert!(!css.contains("width"), "garbage is ignored: {css}");
        let css = presentational_css("tr", &[("height", "0")], &[]);
        assert!(has(&css, "height: 0px"), "a row's height keeps zero: {css}");
    }

    #[test]
    fn bgcolor_uses_the_legacy_colour_rules() {
        let css = presentational_css("td", &[("bgcolor", "#1c3d5a")], &[]);
        assert!(has(&css, "background-color: #1c3d5a"), "{css}");
        let css = presentational_css("td", &[("bgcolor", "ff0000")], &[]);
        assert!(has(&css, "background-color: #ff0000"), "no #: {css}");
        let css = presentational_css("td", &[("bgcolor", "Red")], &[]);
        assert!(has(&css, "background-color: red"), "a named colour: {css}");
        let css = presentational_css("td", &[("bgcolor", "")], &[]);
        assert!(!css.contains("background"), "empty is ignored: {css}");
    }

    #[test]
    fn border_cellspacing_and_bordercolor_style_the_table() {
        let css = presentational_css("table", &[("border", "2")], &[]);
        assert!(has(&css, "border-top-width: 2px"), "{css}");
        assert!(has(&css, "border-left-style: outset"), "{css}");
        for value in ["", "foo", "-1", "1%"] {
            let css = presentational_css("table", &[("border", value)], &[]);
            assert!(
                has(&css, "border-top-width: 1px"),
                "border={value:?} is 1px: {css}"
            );
        }
        for value in ["0", "+0", "-0", "0foo"] {
            let css = presentational_css("table", &[("border", value)], &[]);
            assert!(!css.contains("style"), "border={value:?} draws nothing: {css}");
        }
        let css = presentational_css("table", &[("cellspacing", "4")], &[]);
        assert!(has(&css, "border-spacing: 4px"), "{css}");
        let css = presentational_css("table", &[("bordercolor", "red")], &[]);
        assert!(has(&css, "border-top-color: red"), "{css}");
    }

    #[test]
    fn a_tables_cellpadding_and_border_reach_its_cells() {
        let table = [("cellpadding", "5"), ("border", "1")];
        let css = presentational_css("td", &[], &table);
        assert!(has(&css, "padding-top: 5px"), "{css}");
        assert!(has(&css, "padding-left: 5px"), "{css}");
        assert!(has(&css, "border-top-width: 1px"), "{css}");
        assert!(has(&css, "border-top-style: inset"), "{css}");
        let css = presentational_css("th", &[], &[("border", "0")]);
        assert!(!css.contains("border"), "border=0: no cell border: {css}");
        let css = presentational_css("td", &[], &[("border", "1"), ("bordercolor", "#00ff00")]);
        assert!(has(&css, "border-top-color: #00ff00"), "{css}");
        assert!(
            has(&css, "border-top-style: solid"),
            "with a bordercolor: solid: {css}"
        );
    }

    #[test]
    fn align_valign_and_nowrap() {
        let css = presentational_css("table", &[("align", "center")], &[]);
        assert!(
            has(&css, "margin-left: auto") && has(&css, "margin-right: auto"),
            "{css}"
        );
        let css = presentational_css("table", &[("align", "right")], &[]);
        assert!(has(&css, "float: right"), "{css}");
        let css = presentational_css("td", &[("align", "middle")], &[]);
        assert!(has(&css, "text-align: center"), "{css}");
        let css = presentational_css("p", &[("align", "justify")], &[]);
        assert!(has(&css, "text-align: justify"), "{css}");
        let css = presentational_css("td", &[("valign", "Bottom")], &[]);
        assert!(has(&css, "vertical-align: bottom"), "{css}");
        let css = presentational_css("td", &[("nowrap", "")], &[]);
        assert!(has(&css, "white-space: nowrap"), "{css}");
    }

    #[test]
    fn the_style_attribute_is_not_a_hint() {
        let css = presentational_css("td", &[("style", "width: 9px")], &[]);
        assert!(css.is_empty(), "{css}");
    }
}
