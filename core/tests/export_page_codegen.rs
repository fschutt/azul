//! A whole PAGE (`<html>` with a `<head><style>` and a `<body>`, what the live
//! page export and a pasted HTML document are) → an app project, through the
//! ONE code generator: `azul_core::codegen::dom::lower_xml_page_app` lowers it
//! to the IR, `azul_core::codegen::backend(lang)` prints it. Needs azul-core's
//! `codegen` feature.
//!
//! These pin what the deleted page walkers (`str_to_{rust,c,cpp,python}_code`)
//! were fixed for (B3 audit, scripts/B3_BUILDER_EXPORT.PROGRESS.md §1 row 13)
//! and what `dll/tests/xml_to_rust_compilation.rs` checked of them, now
//! against the shared lowering: a node's `style` attribute is its inline CSS
//! after the stylesheet's rules, mixed text keeps every run exactly once, the
//! semantic constructors (`create_p_with_text`, `create_button(.., aria)`),
//! the C app's reflected data, the C++ callback that releases its RefAny and
//! the Python link without `OptionString`.

use azul_core::{
    codegen::{backend, dom::lower_xml_page_app},
    window::{AzStringPair, StringPairVec},
    xml::{XmlAttributeMap, XmlNode, XmlNodeChild},
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

/// Every file of the app project `lang` prints for `doc`.
fn project(lang: &str, doc: &[XmlNodeChild]) -> Vec<(String, String)> {
    let m = lower_xml_page_app(doc, "T").expect("the page has a body");
    backend(lang)
        .expect(lang)
        .emit_project_files(&m)
        .into_iter()
        .map(|f| (f.path, f.contents))
        .collect()
}

/// The file of `lang`'s app project whose name (without the directory)
/// starts with `stem.` (`ui.` = the module holding `render_ui`).
fn file(lang: &str, doc: &[XmlNodeChild], stem: &str) -> String {
    let files = project(lang, doc);
    files
        .iter()
        .find(|(p, _)| {
            p.rsplit('/')
                .next()
                .is_some_and(|name| name.starts_with(&format!("{stem}.")))
        })
        .map(|(_, c)| c.clone())
        .unwrap_or_else(|| {
            panic!(
                "{lang}: no {stem}.* in {:?}",
                files.iter().map(|(p, _)| p).collect::<Vec<_>>()
            )
        })
}

fn ui(lang: &str, doc: &[XmlNodeChild]) -> String {
    file(lang, doc, "ui")
}

fn has(hay: &str, needle: &str) {
    assert!(hay.contains(needle), "expected `{needle}` in:\n{hay}");
}

#[test]
fn a_node_style_attribute_is_exported_as_its_inline_css_in_every_language() {
    // `get_html_string` (the live-page export) writes each node's computed
    // style into `style="…"`; an export that only matched `<style>` rules
    // made an unstyled app. Newlines collapse: a newline inside a C / C++ /
    // Python string literal would not compile.
    let d = page(
        "",
        vec![elem(node(
            "div",
            &[("style", "color: red;\n    padding: 4px;")],
            vec![],
        ))],
    );
    has(
        &ui("rust", &d),
        ".with_css(azul::str::String::from(\"color: red; padding: 4px;\"))",
    );
    has(
        &ui("c", &d),
        "AzString_copyFromBytes((const uint8_t*)\"color: red; padding: 4px;\", 0, 25)",
    );
    has(
        &ui("cpp", &d),
        "AzString_copyFromBytes(reinterpret_cast<const uint8_t*>(\"color: red; padding: 4px;\"), 0, 25)",
    );
    has(&ui("python", &d), ".with_css(\"color: red; padding: 4px;\")");
}

#[test]
fn a_style_attribute_comes_after_the_matching_stylesheet_rules_so_it_wins() {
    let d = page(
        ".card { margin-top: 1px; }",
        vec![elem(node(
            "div",
            &[("class", "card"), ("style", "color: red")],
            vec![],
        ))],
    );
    let rust = ui("rust", &d);
    let at_rule = rust.find("margin-top").expect("the matched rule is inlined");
    let at_attr = rust.find("color: red").expect("the style attribute is inlined");
    assert!(at_rule < at_attr, "rule first, attribute last:\n{rust}");
}

#[test]
fn a_node_with_text_and_element_children_keeps_every_text_run_exactly_once() {
    // `<p>Before <span>mid</span> after</p>`: an old walker baked the text
    // into the constructor and then emitted the children again.
    let d = page(
        "",
        vec![elem(node(
            "p",
            &[],
            vec![
                txt("Before "),
                elem(node("span", &[], vec![txt("mid")])),
                txt(" after"),
            ],
        ))],
    );
    for lang in ["rust", "c", "cpp", "python"] {
        let code = ui(lang, &d);
        for run in ["Before", "mid", "after"] {
            assert_eq!(code.matches(run).count(), 1, "{lang}: `{run}` once in:\n{code}");
        }
    }
}

#[test]
fn a_paragraph_with_only_text_is_built_by_its_with_text_constructor_in_every_language() {
    let d = page("", vec![elem(node("p", &[], vec![txt("hi")]))]);
    has(
        &ui("rust", &d),
        "Dom::create_p_with_text(azul::str::String::from(\"hi\"))",
    );
    has(
        &ui("c", &d),
        "AzDom_createPWithText(AzString_copyFromBytes((const uint8_t*)\"hi\", 0, 2))",
    );
    has(
        &ui("cpp", &d),
        "AzDom_createPWithText(AzString_copyFromBytes(reinterpret_cast<const uint8_t*>(\"hi\"), 0, 2))",
    );
    has(&ui("python", &d), "Dom.create_p_with_text(\"hi\")");
    // The text is consumed by the constructor, not emitted again as a child.
    assert!(!ui("rust", &d).contains("create_text_do_not_use_without_block_level_wrapper"));
}

#[test]
fn a_button_with_an_aria_label_keeps_its_accessible_name_in_every_language() {
    let d = page(
        "",
        vec![elem(node("button", &[("aria-label", "Go")], vec![txt("Go")]))],
    );
    has(
        &ui("rust", &d),
        "Dom::create_button(azul::str::String::from(\"Go\"), \
         SmallAriaInfo::label(azul::str::String::from(\"Go\")))",
    );
    has(
        &ui("python", &d),
        "Dom.create_button(\"Go\", SmallAriaInfo.label(\"Go\"))",
    );
    has(&ui("c", &d), "AzDom_createButton(");
    has(&ui("c", &d), "AzSmallAriaInfo_label(");
}

#[test]
fn an_empty_body_is_still_an_app_with_a_render_function_and_a_main() {
    let d = page("", vec![]);
    has(&ui("rust", &d), "pub fn render_ui() -> Dom {");
    has(&ui("rust", &d), "Dom::create_body()");
    has(&file("rust", &d, "main"), "fn main()");
}

#[test]
fn the_exported_c_app_gives_its_refany_a_destructor() {
    // `AzRefAny_newC(…, NULL, 0, 0)`: RefCount::drop calls the destructor
    // unconditionally (core/src/refany.rs), so the app crashed on exit.
    let c = file("c", &page("", vec![]), "main");
    assert!(!c.contains("NULL, 0, 0)"), "NULL destructor:\n{c}");
    has(&c, "AZ_REFLECT(");
    has(&c, "_upcast(");
}

#[test]
fn the_exported_cpp_layout_callback_releases_the_refany_it_owns() {
    // The framework hands a callback an OWNED AzRefAny
    // (examples/cpp/cpp20/hello-world.cpp adopts it into azul::RefAny).
    let cpp = file("cpp", &page("", vec![]), "main");
    has(&cpp, "azul::RefAny adopted(data);");
}

#[test]
fn the_exported_python_app_builds_a_link_without_option_string_constructors() {
    // The Python binding has no `OptionString.some` / `.none`, so
    // `create_a_no_a11y(href, OptionString.some(..))` raised at run time.
    let d = page(
        "",
        vec![elem(node(
            "a",
            &[("href", "https://azul.rs")],
            vec![txt("Docs")],
        ))],
    );
    let py = ui("python", &d);
    assert!(!py.contains("OptionString"), "python:\n{py}");
    has(
        &py,
        "Dom.create_a(\"https://azul.rs\", \"Docs\", SmallAriaInfo.label(\"Docs\"))",
    );
}
