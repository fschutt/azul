//! `compile_xml_fragment` / `compile_xml_fragment_app`: a DOM fragment (a
//! builder subtree, a component template) compiled to a render function in
//! Rust, C, C++ and Python — AzBuilder's "Subtree → code" and
//! "Component → code".
//!
//! Golden files live in `core/tests/golden/dom_fragment/`. They are the exact
//! expected output; `AZ_BLESS=1 cargo test -p azul-core --test
//! dom_fragment_codegen` rewrites them from the current output (review the
//! diff, then compile them: `scripts/debugger-ui/compile-export-goldens.sh`).

use std::{fs, path::PathBuf};

use azul_core::{
    window::{AzStringPair, StringPairVec},
    xml::{
        compile_xml_fragment, compile_xml_fragment_app, CompileTarget, FragmentParam,
        XmlAttributeMap, XmlNode, XmlNodeChild,
    },
};
use azul_css::AzString;

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

const TARGETS: [(CompileTarget, &str); 4] = [
    (CompileTarget::Rust, "rs"),
    (CompileTarget::C, "c"),
    (CompileTarget::Cpp, "cpp"),
    (CompileTarget::Python, "py"),
];

fn golden(name: &str, actual: &str) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden/dom_fragment")
        .join(name);
    if std::env::var_os("AZ_BLESS").is_some() {
        fs::create_dir_all(path.parent().expect("golden dir")).expect("create golden dir");
        fs::write(&path, actual).expect("write golden");
        return;
    }
    let expected = fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "no golden file {} — run with AZ_BLESS=1, review it, commit it",
            path.display()
        )
    });
    assert!(
        actual == expected,
        "{name} differs from its golden file ({}); AZ_BLESS=1 rewrites it.\n--- got ---\n{actual}\n--- \
         expected ---\n{expected}",
        path.display()
    );
}

fn rust(nodes: &[XmlNodeChild], css: &str, params: Option<&[FragmentParam]>) -> String {
    compile_xml_fragment(nodes, css, &CompileTarget::Rust, "render_x", params)
        .expect("compiles")
        .function
}

// ── the representative trees ──

/// A builder document subtree: an element with a class and an inline style,
/// a heading, mixed inline content, a link and a button.
fn card() -> Vec<XmlNodeChild> {
    vec![el(
        "div",
        &[("class", "card"), ("style", "padding: 8px")],
        vec![
            el("h1", &[], vec![txt("Title")]),
            el("p", &[], vec![txt("Hello "), el("b", &[], vec![txt("world")])]),
            el("a", &[("href", "https://azul.rs")], vec![txt("Docs")]),
            el("button", &[], vec![txt("Go")]),
        ],
    )]
}

/// A converted component's template: whole-value placeholders, a placeholder
/// in an attribute, and one mixed into literal text.
fn card_template() -> Vec<XmlNodeChild> {
    vec![el(
        "div",
        &[("class", "card")],
        vec![
            el("h2", &[], vec![txt("{title}")]),
            el("p", &[], vec![txt("{text}")]),
            el("a", &[("href", "{href}")], vec![txt("Read more")]),
            el("span", &[], vec![txt("by {author}")]),
        ],
    )]
}

fn card_params() -> Vec<FragmentParam> {
    vec![
        FragmentParam::new("title", "Hello"),
        FragmentParam::new("text", "Some text"),
        FragmentParam::new("href", "https://azul.rs"),
        FragmentParam::new("author", "me"),
    ]
}

// ── goldens ──

#[test]
fn a_document_subtree_compiles_to_one_render_function_per_language() {
    for (target, ext) in TARGETS {
        let f = compile_xml_fragment(&card(), "", &target, "render_card", None).expect("compiles");
        golden(&format!("card.{ext}"), &f.source());
    }
}

#[test]
fn a_component_template_compiles_to_a_function_of_its_parameters_per_language() {
    let params = card_params();
    for (target, ext) in TARGETS {
        let f = compile_xml_fragment(&card_template(), "", &target, "render_card", Some(&params))
            .expect("compiles");
        golden(&format!("card_template.{ext}"), &f.source());
    }
}

#[test]
fn a_page_body_compiles_to_a_runnable_app_per_language() {
    let body = vec![el(
        "body",
        &[],
        vec![
            el("h1", &[], vec![txt("My App")]),
            el("p", &[], vec![txt("Hello")]),
        ],
    )];
    for (target, ext) in TARGETS {
        let app = compile_xml_fragment_app(&body, "", &target, "My App").expect("compiles");
        golden(&format!("app.{ext}"), &app);
    }
}

// ── behaviour ──

#[test]
fn every_language_names_the_parameters_after_the_placeholders() {
    let params = card_params();
    for (target, _) in TARGETS {
        let f = compile_xml_fragment(&card_template(), "", &target, "render_card", Some(&params))
            .expect("compiles");
        assert_eq!(f.param_idents, ["title", "text", "href", "author"], "{target:?}");
        assert_eq!(f.fn_name, "render_card");
        assert!(!f.function.contains("{title}"), "{target:?}:\n{}", f.function);
    }
}

#[test]
fn a_stylesheet_rule_that_matches_a_node_becomes_that_nodes_inline_css_only() {
    let nodes = vec![el(
        "div",
        &[("class", "card")],
        vec![el("p", &[("class", "note")], vec![txt("x")])],
    )];
    let out = rust(&nodes, ".card { margin-top: 3px; } .other { margin-top: 9px; }", None);
    assert_eq!(out.matches(".with_css(").count(), 1, "only the div:\n{out}");
    assert!(out.contains("margin-top: 3px"), "{out}");
    assert!(!out.contains("9px"), "a rule for another class stays out:\n{out}");
}

#[test]
fn a_descendant_rule_reaches_into_the_fragment() {
    let nodes = vec![el(
        "div",
        &[("class", "card")],
        vec![el("p", &[], vec![txt("x")])],
    )];
    let out = rust(&nodes, ".card p { margin-top: 5px; }", None);
    let p_at = out.find("create_p_with_text").expect("the p");
    let css_at = out.find("margin-top: 5px").expect("the rule");
    assert!(css_at > p_at, "the rule lands on the <p>, not the div:\n{out}");
}

#[test]
fn braces_are_text_in_plain_markup_and_escapes_in_a_template() {
    let nodes = vec![el("p", &[], vec![txt("a {{b}} {x}")])];
    assert!(
        rust(&nodes, "", None).contains("Dom::create_p_with_text(\"a {{b}} {x}\")"),
        "plain markup keeps every brace"
    );

    let params = [FragmentParam::new("x", "")];
    let nodes = vec![el("p", &[], vec![txt("{{b}} {x}")])];
    let out = rust(&nodes, "", Some(&params));
    assert!(out.contains("format!(\"{{b}} {x}\")"), "{out}");
    let py = compile_xml_fragment(&nodes, "", &CompileTarget::Python, "f", Some(&params))
        .expect("compiles")
        .function;
    assert!(py.contains("f\"{{b}} {x}\""), "{py}");

    // A `{name}` that is no parameter stays literal text.
    let nodes = vec![el("p", &[], vec![txt("{nope}")])];
    assert!(rust(&nodes, "", Some(&params)).contains("\"{nope}\""));
}

#[test]
fn a_keyword_parameter_gets_an_underscore_in_the_languages_that_reserve_it() {
    let params = [
        FragmentParam::new("for", "email"),
        FragmentParam::new("text", "Email"),
    ];
    let nodes = vec![el("label", &[("for", "{for}")], vec![txt("{text}")])];
    let r = compile_xml_fragment(&nodes, "", &CompileTarget::Rust, "f", Some(&params)).unwrap();
    assert_eq!(r.param_idents, ["for_", "text"]);
    assert!(
        r.function.contains("Dom::create_label_no_a11y(for_, text)"),
        "{}",
        r.function
    );
    let c = compile_xml_fragment(&nodes, "", &CompileTarget::C, "f", Some(&params)).unwrap();
    assert!(c.function.contains("const char* for_"), "{}", c.function);
    let py = compile_xml_fragment(&nodes, "", &CompileTarget::Python, "f", Some(&params)).unwrap();
    assert!(py.function.starts_with("def f(for_=\"email\", text=\"Email\"):"), "{}", py.function);
}

#[test]
fn a_parameter_the_markup_never_shows_is_marked_used_so_it_does_not_warn() {
    // `title` is an attribute no constructor takes.
    let params = [FragmentParam::new("tip", "")];
    let nodes = vec![el("p", &[("title", "{tip}")], vec![txt("x")])];
    let r = rust(&nodes, "", Some(&params));
    assert!(r.contains("let _ = tip;"), "{r}");
    let c = compile_xml_fragment(&nodes, "", &CompileTarget::C, "f", Some(&params)).unwrap();
    assert!(c.function.contains("(void)tip;"), "{}", c.function);
}

#[test]
fn several_roots_are_wrapped_in_one_div() {
    let nodes = vec![
        el("p", &[], vec![txt("a")]),
        el("p", &[], vec![txt("b")]),
    ];
    let out = rust(&nodes, "", None);
    assert!(out.contains("Dom::create_div()"), "{out}");
    assert_eq!(out.matches("create_p_with_text").count(), 2, "{out}");
}

#[test]
fn document_plumbing_inside_a_fragment_is_not_exported() {
    let nodes = vec![el(
        "div",
        &[],
        vec![
            el("style", &[], vec![txt(".x { color: red }")]),
            el("script", &[], vec![txt("alert(1)")]),
            el("p", &[], vec![txt("kept")]),
        ],
    )];
    let out = rust(&nodes, "", None);
    assert!(!out.contains("create_style") && !out.contains("create_script"), "{out}");
    assert!(!out.contains("alert"), "{out}");
    assert!(out.contains("kept"), "{out}");
}

#[test]
fn quotes_backslashes_and_newlines_are_escaped_per_language() {
    let nodes = vec![el("pre", &[], vec![txt("say \"hi\" \\ now\nbye")])];
    for (target, _) in TARGETS {
        let f = compile_xml_fragment(&nodes, "", &target, "f", None).unwrap().function;
        assert!(f.contains("say \\\"hi\\\" \\\\ now\\nbye"), "{target:?}:\n{f}");
    }
}

#[test]
fn inline_content_keeps_the_space_between_a_text_and_the_element_after_it() {
    let out = rust(&card(), "", None);
    assert!(
        out.contains("create_text_do_not_use_without_block_level_wrapper(\"Hello \")"),
        "{out}"
    );
}

#[test]
fn the_c_app_reflects_its_data_instead_of_building_a_refany_with_a_null_destructor() {
    let body = vec![el("body", &[], vec![el("p", &[], vec![txt("x")])])];
    let c = compile_xml_fragment_app(&body, "", &CompileTarget::C, "T").unwrap();
    assert!(c.contains("AZ_REFLECT(AppData, AppData_destructor);"), "{c}");
    assert!(!c.contains("AzRefAny_newC"), "{c}");
}

#[test]
fn a_fragment_that_is_not_a_body_is_put_into_one_by_the_app() {
    let nodes = vec![el("p", &[], vec![txt("x")])];
    let r = compile_xml_fragment_app(&nodes, "", &CompileTarget::Rust, "T").unwrap();
    assert!(r.contains("Dom::create_body().with_child(render_ui())"), "{r}");
    let c = compile_xml_fragment_app(&nodes, "", &CompileTarget::C, "T").unwrap();
    assert!(c.contains("AzDom_addChild(&body, render_ui());"), "{c}");
}
