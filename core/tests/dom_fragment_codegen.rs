//! The DOM-fragment LOWERING (`azul_core::codegen::dom::lower_xml_fragment`,
//! `lower_xml_fragment_app`): a builder subtree, a component template or a
//! page body → the codegen IR (`azul_css::codegen::ir`) that every binding
//! language's printer turns into source. Needs azul-core's `codegen` feature.
//!
//! The printers are tested in ONE place, B2's golden harness
//! (`css/tests/codegen_goldens.rs`, cases `dom_card` / `dom_library` /
//! `dom_app` built in `css/tests/codegen_cases/mod.rs`). These tests pin
//! that the lowering produces exactly those IR values from markup, and the
//! lowering decisions (constructors, CSS, placeholders, whitespace) in IR
//! terms, independent of any language.

use azul_core::{
    codegen::{
        backend,
        dom::{lower_xml_fragment, lower_xml_fragment_app, FragmentParam},
    },
    window::{AzStringPair, StringPairVec},
    xml::{XmlAttributeMap, XmlNode, XmlNodeChild},
};
use azul_css::{
    codegen::{
        all_backends,
        ir::{AppSpec, Expr, Ident, Item, ItemParam, Module},
    },
    AzString,
};

// ── tree helpers ──

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

fn el(tag: &str, attrs: &[(&str, &str)], children: Vec<XmlNodeChild>) -> XmlNodeChild {
    XmlNodeChild::Element(node(tag, attrs, children))
}

// ── IR helpers (the same spelling as css/tests/codegen_cases) ──

fn dom(m: &str, args: Vec<Expr>) -> Expr {
    Expr::call("Dom", m, args)
}

fn with(recv: Expr, m: &str, args: Vec<Expr>) -> Expr {
    Expr::method(recv, "Dom", m, args)
}

fn value_of(m: &Module) -> &Expr {
    &m.items[0].value
}

/// The one item's value, printed flat for substring checks.
fn ir(nodes: &[XmlNodeChild], css: &str, params: Option<&[FragmentParam]>) -> Expr {
    lower_xml_fragment(nodes, css, "render_x", params, Vec::new())
        .items
        .remove(0)
        .value
}

// ── the cases the printers are golden-tested with ──

#[test]
fn a_converted_components_template_lowers_to_the_ir_the_printers_are_tested_with() {
    // css/tests/codegen_cases::dom_card_module, from markup
    let template = vec![el(
        "div",
        &[("class", "card"), ("style", "padding: 8px")],
        vec![
            el("h2", &[], vec![txt("{title}")]),
            el("p", &[], vec![txt("{text}")]),
            el("a", &[("href", "{href}")], vec![txt("Read more")]),
            el("span", &[], vec![txt("by {author}")]),
        ],
    )];
    let params = [
        FragmentParam::new("title", "Hello"),
        FragmentParam::new("text", "Some text"),
        FragmentParam::new("href", "https://azul.rs"),
        FragmentParam::new("author", "me"),
    ];
    let got = lower_xml_fragment(
        &template,
        "",
        "render_card",
        Some(&params),
        vec!["`user:card`: its texts and its link are parameters".to_string()],
    );

    let s = Expr::str;
    let p = Expr::param;
    let mut value = dom("create_div", vec![]);
    value = with(value, "with_css", vec![s("padding: 8px")]);
    value = with(value, "with_class", vec![s("card")]);
    value = with(value, "with_child", vec![dom("create_h2_with_text", vec![p("title")])]);
    value = with(value, "with_child", vec![dom("create_p_with_text", vec![p("text")])]);
    value = with(
        value,
        "with_child",
        vec![dom(
            "create_a",
            vec![
                p("href"),
                s("Read more"),
                Expr::call("SmallAriaInfo", "label", vec![s("Read more")]),
            ],
        )],
    );
    value = with(
        value,
        "with_child",
        vec![dom(
            "create_span_with_text",
            vec![Expr::concat(vec![s("by "), p("author")])],
        )],
    );
    let want = Module {
        items: vec![Item {
            name: Ident::from_text("render_card"),
            doc: vec!["`user:card`: its texts and its link are parameters".to_string()],
            ty: "Dom".to_string(),
            params: vec![
                ItemParam::string("title", "Hello"),
                ItemParam::string("text", "Some text"),
                ItemParam::string("href", "https://azul.rs"),
                ItemParam::string("author", "me"),
            ],
            value,
        }],
        ..Module::default()
    };
    assert_eq!(got, want);
}

#[test]
fn a_page_body_lowers_to_an_app_whose_body_is_the_window() {
    // css/tests/codegen_cases::dom_app_module, from markup
    let body = vec![el(
        "body",
        &[],
        vec![
            el("h1", &[], vec![txt("My App")]),
            el("p", &[], vec![txt("Hello")]),
        ],
    )];
    let got = lower_xml_fragment_app(&body, "", "My App");
    let value = with(
        with(
            dom("create_body", vec![]),
            "with_child",
            vec![dom("create_h1_with_text", vec![Expr::str("My App")])],
        ),
        "with_child",
        vec![dom("create_p_with_text", vec![Expr::str("Hello")])],
    );
    assert_eq!(value_of(&got), &value);
    assert_eq!(got.items[0].name, Ident::from_text("render_ui"));
    assert!(got.items[0].params.is_empty());
    assert_eq!(
        got.app,
        Some(AppSpec {
            title: "My App".to_string(),
            root: Ident::from_text("render_ui"),
            is_body: true,
        })
    );

    // Anything but a body goes INTO one.
    let p = lower_xml_fragment_app(&[el("p", &[], vec![txt("x")])], "", "T");
    assert!(!p.app.expect("an app").is_body);
}

// ── lowering decisions ──

#[test]
fn a_matching_stylesheet_rule_comes_before_the_style_attribute_in_with_css() {
    let nodes = vec![el(
        "div",
        &[("class", "card"), ("style", "color: red")],
        vec![el("p", &[("class", "note")], vec![txt("x")])],
    )];
    let flat = format!(
        "{:?}",
        ir(&nodes, ".card { margin-top: 3px; } .other { margin-top: 9px; }", None)
    );
    let rule = flat.find("margin-top: 3px").expect("the matching rule");
    let attr = flat.find("color: red").expect("the style attribute");
    assert!(rule < attr, "{flat}");
    assert!(!flat.contains("9px"), "a rule for another class stays out: {flat}");
    assert_eq!(flat.matches("with_css").count(), 1, "{flat}");
}

#[test]
fn braces_are_text_in_plain_markup_and_placeholders_in_a_template() {
    let plain = ir(&[el("p", &[], vec![txt("a {{b}} {x}")])], "", None);
    assert_eq!(
        plain,
        dom("create_p_with_text", vec![Expr::str("a {{b}} {x}")])
    );

    let params = [FragmentParam::new("x", "")];
    let t = ir(&[el("p", &[], vec![txt("{{b}} {x}")])], "", Some(&params));
    assert_eq!(
        t,
        dom(
            "create_p_with_text",
            vec![Expr::concat(vec![Expr::str("{b} "), Expr::param("x")])]
        )
    );
    // An unknown `{name}` stays literal text.
    let unknown = ir(&[el("p", &[], vec![txt("{nope}")])], "", Some(&params));
    assert_eq!(unknown, dom("create_p_with_text", vec![Expr::str("{nope}")]));
}

#[test]
fn a_link_is_always_create_a_with_its_text_as_its_accessible_name() {
    let label = |s: &str| Expr::call("SmallAriaInfo", "label", vec![Expr::str(s)]);
    assert_eq!(
        ir(&[el("a", &[("href", "/x")], vec![txt("Go")])], "", None),
        dom("create_a", vec![Expr::str("/x"), Expr::str("Go"), label("Go")])
    );
    // No text: the href names it.
    assert_eq!(
        ir(&[el("a", &[("href", "/x")], vec![])], "", None),
        dom("create_a", vec![Expr::str("/x"), Expr::str("/x"), label("/x")])
    );
}

#[test]
fn several_roots_go_into_one_div_and_document_plumbing_is_left_out() {
    let two = ir(
        &[el("p", &[], vec![txt("a")]), el("p", &[], vec![txt("b")])],
        "",
        None,
    );
    assert_eq!(
        two,
        with(
            with(
                dom("create_div", vec![]),
                "with_child",
                vec![dom("create_p_with_text", vec![Expr::str("a")])]
            ),
            "with_child",
            vec![dom("create_p_with_text", vec![Expr::str("b")])]
        )
    );
    let plumbing = format!(
        "{:?}",
        ir(
            &[el(
                "div",
                &[],
                vec![
                    el("style", &[], vec![txt(".x { color: red }")]),
                    el("script", &[], vec![txt("alert(1)")]),
                    el("p", &[], vec![txt("kept")]),
                ],
            )],
            "",
            None,
        )
    );
    assert!(!plumbing.contains("alert") && !plumbing.contains("create_style"), "{plumbing}");
    assert!(plumbing.contains("kept"), "{plumbing}");
}

#[test]
fn inline_content_keeps_the_space_before_the_next_element() {
    let e = ir(
        &[el("p", &[], vec![txt("  Hello  "), el("b", &[], vec![txt("you")])])],
        "",
        None,
    );
    assert_eq!(
        e,
        with(
            with(
                dom("create_p", vec![]),
                "with_child",
                vec![dom(
                    "create_text_do_not_use_without_block_level_wrapper",
                    vec![Expr::str("Hello ")]
                )]
            ),
            "with_child",
            vec![dom("create_b_with_text", vec![Expr::str("you")])]
        )
    );
}

// ── printing through B2's printers ──

#[test]
fn every_language_prints_a_fragment_or_says_why_it_cannot() {
    let nodes = vec![el("p", &[("class", "x")], vec![txt("{text}")])];
    let params = [FragmentParam::new("text", "Hi")];
    let m = lower_xml_fragment(&nodes, "", "render_x", Some(&params), Vec::new());
    for b in all_backends() {
        let code = backend(b.lang())
            .expect("every listed language has a code generator")
            .emit_module(&m);
        assert!(!code.trim().is_empty(), "{}", b.lang());
    }
    let rust = backend("rust").expect("rust").emit_module(&m);
    assert!(rust.contains("pub fn render_x(text: &str) -> Dom {"), "{rust}");
    let err = backend("klingon").err().expect("no such language");
    assert!(err.contains("available:"), "{err}");
}

#[test]
fn a_c_app_reflects_its_data_instead_of_a_refany_with_a_null_destructor() {
    let body = vec![el("body", &[], vec![el("p", &[], vec![txt("x")])])];
    let files = backend("c")
        .expect("c")
        .emit_project_files(&lower_xml_fragment_app(&body, "", "T"));
    let main = files
        .iter()
        .find(|f| f.path == "main.c")
        .expect("main.c");
    assert!(main.contents.contains("AZ_REFLECT(AppData, AppData_destructor);"), "{}", main.contents);
    assert!(!main.contents.contains("AzRefAny_newC"), "{}", main.contents);
}
