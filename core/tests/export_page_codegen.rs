//! The page exporters (`str_to_{rust,c,cpp,python}_code`, behind the debug
//! server's `export_code` / Export > Code) against what the generated app
//! needs today.
//!
//! Each test is a defect the B3 audit found in code that type-checked but did
//! not work (scripts/B3_BUILDER_EXPORT.PROGRESS.md §1 row 13):
//! the live page carries every node's style in a `style` attribute that no
//! walker read, the C app's RefAny had a NULL destructor, the C++ layout
//! callback never released its RefAny, and the Python app called
//! `OptionString` constructors the binding does not have.

use azul_core::{
    window::{AzStringPair, StringPairVec},
    xml::{
        str_to_c_code, str_to_cpp_code, str_to_python_code, str_to_rust_code, ComponentMap,
        XmlAttributeMap, XmlNode, XmlNodeChild,
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

fn elem(n: XmlNode) -> XmlNodeChild {
    XmlNodeChild::Element(n)
}

/// `<html><head><style>{css}</style></head><body>{children}</body></html>`
fn page(css: &str, body: Vec<XmlNodeChild>) -> Vec<XmlNodeChild> {
    let style = node("style", &[], vec![txt(css)]);
    let head = node("head", &[], vec![elem(style)]);
    vec![elem(node(
        "html",
        &[],
        vec![elem(head), elem(node("body", &[], body))],
    ))]
}

#[test]
fn a_node_style_attribute_is_exported_as_its_inline_css_in_every_language() {
    // `get_html_string` (the live-page export) writes each node's computed
    // style into `style="…"`; a walker that only matches `<style>` rules
    // exports an unstyled app.
    let map = ComponentMap::with_builtin();
    let d = page(
        "",
        vec![elem(node(
            "div",
            &[("style", "color: red;\n    padding: 4px;")],
            vec![],
        ))],
    );

    let rust = str_to_rust_code(&d, "", &map).expect("rust");
    assert!(
        rust.contains(".with_css(\"color: red; padding: 4px;\")"),
        "rust:\n{rust}"
    );
    let c = str_to_c_code(&d, &map).expect("c");
    assert!(
        c.contains("AzDom_withCss(n1, AZ_STR(\"color: red; padding: 4px;\"))"),
        "c:\n{c}"
    );
    let cpp = str_to_cpp_code(&d, &map).expect("cpp");
    assert!(
        cpp.contains(".with_css(String(\"color: red; padding: 4px;\"))"),
        "cpp:\n{cpp}"
    );
    let py = str_to_python_code(&d, &map).expect("python");
    assert!(
        py.contains(".with_css(\"color: red; padding: 4px;\")"),
        "python:\n{py}"
    );
}

#[test]
fn a_style_attribute_comes_after_the_matching_stylesheet_rules_so_it_wins() {
    let map = ComponentMap::with_builtin();
    let d = page(
        ".card { margin-top: 1px; }",
        vec![elem(node(
            "div",
            &[("class", "card"), ("style", "color: red")],
            vec![],
        ))],
    );
    let rust = str_to_rust_code(&d, "", &map).expect("rust");
    let at_rule = rust.find("margin-top").expect("the matched rule is inlined");
    let at_attr = rust.find("color: red").expect("the style attribute is inlined");
    assert!(at_rule < at_attr, "rule first, attribute last:\n{rust}");
}

#[test]
fn the_exported_c_app_gives_its_refany_a_destructor() {
    // `AzRefAny_newC(…, NULL, 0, 0)`: RefCount::drop calls the destructor
    // unconditionally (core/src/refany.rs), so the app crashed on exit.
    let map = ComponentMap::with_builtin();
    let c = str_to_c_code(&page("", vec![]), &map).expect("c");
    assert!(!c.contains("NULL, 0, 0)"), "NULL destructor:\n{c}");
    assert!(c.contains("AZ_REFLECT("), "a reflected app data type:\n{c}");
    assert!(c.contains("_upcast("), "built through the reflection:\n{c}");
}

#[test]
fn the_exported_cpp_layout_callback_releases_the_refany_it_owns() {
    // The framework hands a callback an OWNED AzRefAny
    // (examples/cpp/cpp20/hello-world.cpp adopts it into azul::RefAny).
    let map = ComponentMap::with_builtin();
    let cpp = str_to_cpp_code(&page("", vec![]), &map).expect("cpp");
    assert!(cpp.contains("RefAny adopted(data);"), "cpp:\n{cpp}");
    assert!(
        cpp.contains("ffi::Dom render(ffi::RefAny data, ffi::LayoutCallbackInfo info)"),
        "cpp:\n{cpp}"
    );
}

#[test]
fn the_exported_python_app_builds_a_link_without_option_string_constructors() {
    // The Python binding has no `OptionString.some` / `.none` (only
    // `OptionString.None()`, which is not even valid syntax), so
    // `create_a_no_a11y(href, OptionString.some(..))` raised at run time.
    let map = ComponentMap::with_builtin();
    let d = page(
        "",
        vec![elem(node(
            "a",
            &[("href", "https://azul.rs")],
            vec![txt("Docs")],
        ))],
    );
    let py = str_to_python_code(&d, &map).expect("python");
    assert!(!py.contains("OptionString"), "python:\n{py}");
    assert!(
        py.contains(
            "azul.Dom.create_a(\"https://azul.rs\", \"Docs\", azul.SmallAriaInfo.label(\"Docs\"))"
        ),
        "python:\n{py}"
    );
}
