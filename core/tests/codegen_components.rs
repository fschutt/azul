//! COMPONENT-AWARE lowering (`azul_core::codegen::dom`): a component instance
//! in the markup is a CALL of that component's function, never its tree
//! inlined, and every component used is lowered ONCE into its own item.
//!
//! How code calls a component is part of the component itself,
//! language-neutral: `ComponentDef::codegen` (`ComponentCodegen`) replaced the
//! per-language string hook `compile_fn` / `CompileTarget`:
//!
//! - `RenderFunction`: its own render function `render_<name>(<fields>)`,
//!   defined from its template (or what it renders);
//! - `Element`: an HTML element of the builtin library (`Dom::create_<tag>`);
//! - `Call(..)`: a constructor in api.json vocabulary, a widget
//!   (`Button::create(label).dom()`).
//!
//! Needs azul-core's `codegen` feature.

use azul_core::{
    codegen::{
        backend,
        dom::{
            lower_component_library, lower_components_fragment, ComponentMarkup, Components,
        },
    },
    window::{AzStringPair, StringPairVec},
    xml::{
        register_builtin_components, user_defined_render_fn, ComponentCallCodegen,
        ComponentCodegen, ComponentDataField, ComponentDataModel, ComponentDef,
        ComponentDefaultValue, ComponentFieldType, ComponentId, ComponentLibrary, ComponentMap,
        ComponentSource, OptionComponentDefaultValue, XmlAttributeMap, XmlNode, XmlNodeChild,
    },
};
use azul_css::{
    codegen::ir::{Expr, Ident, Item, ItemParam, Module},
    AzString, StringVec,
};

// ── markup helpers ──

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

// ── IR helpers ──

fn dom(m: &str, args: Vec<Expr>) -> Expr {
    Expr::call("Dom", m, args)
}

fn with(recv: Expr, m: &str, args: Vec<Expr>) -> Expr {
    Expr::method(recv, "Dom", m, args)
}

// ── the component map: two user components, one nested in the other, and a widget ──

fn string_field(name: &str, default: &str) -> ComponentDataField {
    ComponentDataField {
        name: AzString::from(name),
        field_type: ComponentFieldType::String,
        default_value: OptionComponentDefaultValue::Some(ComponentDefaultValue::String(
            AzString::from(default),
        )),
        required: false,
        description: AzString::from(""),
    }
}

fn def(
    library: &str,
    name: &str,
    display: &str,
    fields: Vec<ComponentDataField>,
    codegen: ComponentCodegen,
) -> ComponentDef {
    ComponentDef {
        id: ComponentId::new(library, name),
        display_name: AzString::from(display),
        description: AzString::from(""),
        css: AzString::from(""),
        source: ComponentSource::UserDefined,
        data_model: ComponentDataModel {
            name: AzString::from(format!("{display}Data").as_str()),
            description: AzString::from(""),
            fields: fields.into(),
        },
        render_fn: user_defined_render_fn,
        codegen,
        render_fn_source: None.into(),
    }
}

fn library(name: &str, components: Vec<ComponentDef>) -> ComponentLibrary {
    ComponentLibrary {
        name: AzString::from(name),
        version: AzString::from("0.1.0"),
        description: AzString::from(""),
        components: components.into(),
        exportable: true,
        modifiable: true,
        data_models: Vec::new().into(),
        enum_models: Vec::new().into(),
    }
}

/// builtin + `user:card(title, tag)` (which uses `user:badge`) +
/// `user:badge(text)` + `user:loop` (uses itself) + `widgets:button(label)`.
fn map() -> ComponentMap {
    let button = def(
        "widgets",
        "button",
        "Button",
        vec![string_field("label", "Button")],
        ComponentCodegen::Call(ComponentCallCodegen {
            class: AzString::from("Button"),
            constructor: AzString::from("create"),
            args: StringVec::from_vec(vec![AzString::from("label")]),
            setters: StringPairVec::from_vec(Vec::new()),
            finish: AzString::from("dom"),
        }),
    );
    ComponentMap {
        libraries: vec![
            register_builtin_components(),
            library(
                "user",
                vec![
                    def(
                        "user",
                        "card",
                        "Card",
                        vec![string_field("title", "Hello"), string_field("tag", "New")],
                        ComponentCodegen::RenderFunction,
                    ),
                    def(
                        "user",
                        "badge",
                        "Badge",
                        vec![string_field("text", "New")],
                        ComponentCodegen::RenderFunction,
                    ),
                    def("user", "loop", "Loop", vec![], ComponentCodegen::RenderFunction),
                ],
            ),
            library("widgets", vec![button]),
        ]
        .into(),
    }
}

/// The templates (what the builder's `render_fn_source` holds, parsed): the
/// card uses the badge, passing its own `tag` on.
fn template(d: &ComponentDef) -> Option<Result<ComponentMarkup, String>> {
    let nodes = match d.id.name.as_str() {
        "card" => vec![el(
            "div",
            &[("class", "card")],
            vec![
                el("h2", &[], vec![txt("{title}")]),
                el("user:badge", &[("text", "{tag}")], vec![]),
            ],
        )],
        "badge" => vec![el("span", &[("class", "badge")], vec![txt("{text}")])],
        "loop" => vec![el("div", &[], vec![el("user:loop", &[], vec![])])],
        _ => return None,
    };
    Some(Ok(ComponentMarkup {
        nodes,
        css: String::new(),
        is_template: true,
    }))
}

/// `<body><user:card title="Hi" tag="Beta"/><widgets:button label="OK"/></body>`
fn page() -> Vec<XmlNodeChild> {
    vec![el(
        "body",
        &[],
        vec![
            el("user:card", &[("title", "Hi"), ("tag", "Beta")], vec![]),
            el("widgets:button", &[("label", "OK")], vec![]),
        ],
    )]
}

fn lowered_page() -> Module {
    let map = map();
    let components = Components {
        map: &map,
        template: &template,
    };
    lower_components_fragment(&page(), "", "render_ui", Vec::new(), &components)
}

fn item<'a>(m: &'a Module, name: &str) -> &'a Item {
    m.item(&Ident::from_text(name))
        .unwrap_or_else(|| panic!("no item {name} in {:?}", m.items.iter().map(|i| &i.name).collect::<Vec<_>>()))
}

// ── the IR ──

#[test]
fn a_page_with_nested_user_components_and_a_widget_has_one_function_per_component_and_calls_them() {
    let m = lowered_page();
    // Each component used is lowered once, callees before their callers
    // (C and C++ need a definition before its use), the page last.
    let names: Vec<String> = m.items.iter().map(|i| i.name.snake()).collect();
    assert_eq!(names, ["render_badge", "render_card", "render_ui"]);

    let badge = item(&m, "render_badge");
    assert_eq!(badge.params, vec![ItemParam::string("text", "New")]);
    assert_eq!(
        badge.value,
        with(
            dom("create_span_with_text", vec![Expr::param("text")]),
            "with_class",
            vec![Expr::str("badge")]
        )
    );

    let card = item(&m, "render_card");
    assert_eq!(
        card.params,
        vec![ItemParam::string("title", "Hello"), ItemParam::string("tag", "New")]
    );
    // The nested component is a call inside the call, its argument the
    // card's own parameter.
    assert_eq!(
        card.value,
        with(
            with(
                with(dom("create_div", vec![]), "with_class", vec![Expr::str("card")]),
                "with_child",
                vec![dom("create_h2_with_text", vec![Expr::param("title")])]
            ),
            "with_child",
            vec![Expr::item_call("render_badge", vec![("text", Expr::param("tag"))])]
        )
    );

    let ui = item(&m, "render_ui");
    assert!(ui.params.is_empty());
    assert_eq!(
        ui.value,
        with(
            with(
                dom("create_body", vec![]),
                "with_child",
                vec![Expr::item_call(
                    "render_card",
                    vec![("title", Expr::str("Hi")), ("tag", Expr::str("Beta"))]
                )]
            ),
            "with_child",
            // The widget is its constructor in api.json vocabulary.
            vec![Expr::method(
                Expr::call("Button", "create", vec![Expr::str("OK")]),
                "Button",
                "dom",
                vec![]
            )]
        )
    );
}

#[test]
fn nothing_of_a_component_is_inlined_into_its_caller() {
    let m = lowered_page();
    let flat = |name: &str| format!("{:?}", item(&m, name).value);
    assert!(!flat("render_ui").contains("create_span_with_text"), "{}", flat("render_ui"));
    assert!(!flat("render_ui").contains("create_h2_with_text"), "{}", flat("render_ui"));
    assert!(!flat("render_card").contains("create_span_with_text"), "{}", flat("render_card"));
}

#[test]
fn an_instance_attribute_that_is_not_a_field_styles_the_call_and_its_children_are_appended() {
    let map = map();
    let components = Components {
        map: &map,
        template: &template,
    };
    let nodes = vec![el(
        "user:badge",
        &[("text", "A"), ("class", "wide")],
        vec![el("p", &[], vec![txt("inside")])],
    )];
    let m = lower_components_fragment(&nodes, "", "render_x", Vec::new(), &components);
    assert_eq!(
        item(&m, "render_x").value,
        with(
            with(
                Expr::item_call("render_badge", vec![("text", Expr::str("A"))]),
                "with_class",
                vec![Expr::str("wide")]
            ),
            "with_child",
            vec![dom("create_p_with_text", vec![Expr::str("inside")])]
        )
    );
}

#[test]
fn a_missing_field_is_passed_as_its_default_and_the_text_field_takes_the_instances_text() {
    let map = map();
    let components = Components {
        map: &map,
        template: &template,
    };
    let nodes = vec![el(
        "body",
        &[],
        vec![
            el("user:card", &[], vec![]),
            el("user:badge", &[], vec![txt(" Soon ")]),
        ],
    )];
    let m = lower_components_fragment(&nodes, "", "render_x", Vec::new(), &components);
    let flat = format!("{:?}", item(&m, "render_x").value);
    assert!(
        flat.contains(&format!(
            "{:?}",
            Expr::item_call("render_card", vec![("title", Expr::str("Hello")), ("tag", Expr::str("New"))])
        )),
        "{flat}"
    );
    assert!(
        flat.contains(&format!(
            "{:?}",
            Expr::item_call("render_badge", vec![("text", Expr::str("Soon"))])
        )),
        "{flat}"
    );
}

#[test]
fn a_component_that_uses_itself_is_cut_with_a_note_instead_of_recursing_forever() {
    let map = map();
    let components = Components {
        map: &map,
        template: &template,
    };
    let m = lower_components_fragment(
        &[el("user:loop", &[], vec![])],
        "",
        "render_x",
        Vec::new(),
        &components,
    );
    let lp = item(&m, "render_loop");
    assert_eq!(
        lp.value,
        with(dom("create_div", vec![]), "with_child", vec![dom("create_div", vec![])])
    );
    assert!(
        lp.doc.iter().any(|d| d.contains("user:loop") && d.contains("recursive")),
        "{:?}",
        lp.doc
    );
}

#[test]
fn a_missing_component_is_an_empty_div_with_a_note() {
    let map = map();
    let components = Components {
        map: &map,
        template: &template,
    };
    let m = lower_components_fragment(
        &[el("user:nope", &[], vec![])],
        "",
        "render_x",
        Vec::new(),
        &components,
    );
    let x = item(&m, "render_x");
    assert_eq!(x.value, dom("create_div", vec![]));
    assert!(x.doc.iter().any(|d| d.contains("user:nope")), "{:?}", x.doc);
}

#[test]
fn a_component_without_a_template_is_its_rendering_as_a_function_without_parameters() {
    // No template: `ComponentMarkup::rendered` (its render_fn with its default
    // data, as markup).
    let map = map();
    let components = Components {
        map: &map,
        template: &|_: &ComponentDef| None,
    };
    let m = lower_components_fragment(
        &[el("user:badge", &[("text", "ignored")], vec![])],
        "",
        "render_x",
        Vec::new(),
        &components,
    );
    let badge = item(&m, "render_badge");
    assert!(badge.params.is_empty(), "{:?}", badge.params);
    assert!(
        badge.doc.iter().any(|d| d.contains("default data")),
        "{:?}",
        badge.doc
    );
    assert_eq!(
        item(&m, "render_x").value,
        Expr::item_call("render_badge", vec![])
    );
}

#[test]
fn a_library_is_one_module_of_its_components_and_what_they_use() {
    let map = map();
    let components = Components {
        map: &map,
        template: &template,
    };
    let card = map.get("user", "card").expect("card");
    let mut warnings = Vec::new();
    let m = lower_component_library("user", "0.1.0", &[card], &components, &mut warnings);
    let names: Vec<String> = m.items.iter().map(|i| i.name.snake()).collect();
    assert_eq!(names, ["render_badge", "render_card"]);
    let lib = m.library.expect("a library");
    assert_eq!(lib.name, "user");
    // Only the library's own components are registered.
    assert_eq!(lib.components.len(), 1);
    assert_eq!(lib.components[0].item, Ident::from_text("render_card"));
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[test]
fn every_builtin_component_says_how_code_builds_it() {
    let map = ComponentMap::with_builtin();
    for d in map.all_components() {
        // `builtin:map` names two builtins: HTML's image map (an element)
        // and the structural map (a render function).
        let structural = match d.id.name.as_str() {
            "if" | "for" => true,
            "map" => d.display_name.as_str() != "Image Map",
            _ => false,
        };
        let want = if structural {
            ComponentCodegen::RenderFunction
        } else {
            ComponentCodegen::Element
        };
        assert_eq!(d.codegen, want, "{}", d.id.qualified_name());
    }
}

// ── printed in Rust, C and Python ──

fn print(lang: &str) -> String {
    backend(lang).expect(lang).emit_module(&lowered_page())
}

fn has(hay: &str, needle: &str) {
    assert!(hay.contains(needle), "expected `{needle}` in:\n{hay}");
}

fn before(hay: &str, a: &str, b: &str) {
    let ia = hay.find(a).unwrap_or_else(|| panic!("no `{a}` in:\n{hay}"));
    let ib = hay.find(b).unwrap_or_else(|| panic!("no `{b}` in:\n{hay}"));
    assert!(ia < ib, "`{a}` must come before `{b}`:\n{hay}");
}

#[test]
fn rust_prints_one_function_per_component_and_calls() {
    let rust = print("rust");
    has(&rust, "pub fn render_badge(text: &str) -> Dom {");
    has(&rust, "pub fn render_card(title: &str, tag: &str) -> Dom {");
    has(&rust, ".with_child(render_badge(tag))");
    has(&rust, ".with_child(render_card(\"Hi\", \"Beta\"))");
    has(
        &rust,
        ".with_child(Button::create(azul::str::String::from(\"OK\")).dom())",
    );
    has(&rust, "use azul::widgets::*;");
    assert_eq!(rust.matches("create_span_with_text").count(), 1, "{rust}");
}

#[test]
fn c_prints_one_function_per_component_before_its_callers_and_calls() {
    let c = print("c");
    has(&c, "static AzDom render_badge(const char* text) {");
    has(&c, "static AzDom render_card(const char* title, const char* tag) {");
    has(&c, "render_badge(tag)");
    has(&c, "render_card(\"Hi\", \"Beta\")");
    has(
        &c,
        "AzButton_dom(AzButton_create(AzString_copyFromBytes((const uint8_t*)\"OK\", 0, 2)))",
    );
    before(&c, "static AzDom render_badge(", "static AzDom render_card(");
    before(&c, "static AzDom render_card(", "static AzDom render_ui(");
    assert_eq!(c.matches("AzDom_createSpanWithText").count(), 1, "{c}");
}

#[test]
fn python_prints_one_function_per_component_and_calls() {
    let py = print("python");
    has(&py, "def render_badge(text=\"New\"):");
    has(&py, "def render_card(title=\"Hello\", tag=\"New\"):");
    has(&py, ".with_child(render_badge(tag))");
    has(&py, ".with_child(render_card(\"Hi\", \"Beta\"))");
    has(&py, ".with_child(Button.create(\"OK\").dom())");
    assert_eq!(py.matches("create_span_with_text").count(), 1, "{py}");
}
