//! The project API (`azul_core::codegen::project`): ONE function family that
//! combines the two generators - the DOM (markup / components → a render
//! function or an app) and the CSS (a stylesheet → named styles) - on top of
//! the component API (a component library is a file of render functions
//! plus its registration). AzBuilder's Export > Code, its quick exports and
//! "HTML → DOM (code)" are thin callers of it. Needs azul-core's `codegen`
//! feature.

use azul_core::{
    codegen::{
        dom::{ComponentMarkup, Components},
        project::{html_code, project_files, AppMarkup, CodeMode, ProjectSpec},
    },
    window::{AzStringPair, StringPairVec},
    xml::{
        register_builtin_components, user_defined_render_fn, ComponentCodegen,
        ComponentDataField, ComponentDataModel, ComponentDef, ComponentDefaultValue,
        ComponentFieldType, ComponentId, ComponentLibrary, ComponentMap, ComponentSource,
        OptionComponentDefaultValue, XmlAttributeMap, XmlNode, XmlNodeChild,
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

fn el(tag: &str, attrs: &[(&str, &str)], children: Vec<XmlNodeChild>) -> XmlNodeChild {
    XmlNodeChild::Element(node(tag, attrs, children))
}

fn txt(s: &str) -> XmlNodeChild {
    XmlNodeChild::Text(AzString::from(s))
}

/// builtin + `user:card(title)`, CSS `.card { padding-top: 4px; }`.
fn map() -> ComponentMap {
    let card = ComponentDef {
        id: ComponentId::new("user", "card"),
        display_name: AzString::from("Card"),
        description: AzString::from(""),
        css: AzString::from(".card { padding-top: 4px; }"),
        source: ComponentSource::UserDefined,
        data_model: ComponentDataModel {
            name: AzString::from("CardData"),
            description: AzString::from(""),
            fields: vec![ComponentDataField {
                name: AzString::from("title"),
                field_type: ComponentFieldType::String,
                default_value: OptionComponentDefaultValue::Some(ComponentDefaultValue::String(
                    AzString::from("Hello"),
                )),
                required: false,
                description: AzString::from("The heading"),
            }]
            .into(),
        },
        render_fn: user_defined_render_fn,
        codegen: ComponentCodegen::RenderFunction,
        render_fn_source: None.into(),
    };
    ComponentMap {
        libraries: vec![
            register_builtin_components(),
            ComponentLibrary {
                name: AzString::from("user"),
                version: AzString::from("0.2.0"),
                description: AzString::from(""),
                components: vec![card].into(),
                exportable: true,
                modifiable: true,
                data_models: Vec::new().into(),
                enum_models: Vec::new().into(),
            },
        ]
        .into(),
    }
}

fn template(d: &ComponentDef) -> Option<Result<ComponentMarkup, String>> {
    (d.id.name.as_str() == "card").then(|| {
        Ok(ComponentMarkup {
            nodes: vec![el("div", &[("class", "card")], vec![el("h2", &[], vec![txt("{title}")])])],
            css: d.css.as_str().to_string(),
            is_template: true,
        })
    })
}

/// `<body><user:card title="Hi"/><p class="note">x</p></body>` with a page
/// stylesheet for `.note`.
fn app() -> AppMarkup {
    AppMarkup::Fragment {
        nodes: vec![el(
            "body",
            &[],
            vec![
                el("user:card", &[("title", "Hi")], vec![]),
                el("p", &[("class", "note")], vec![txt("x")]),
            ],
        )],
        css: ".note { margin-top: 2px; }".to_string(),
    }
}

fn files(lang: &str) -> (Vec<(String, String)>, Vec<String>) {
    let map = map();
    let components = Components {
        map: &map,
        template: &template,
    };
    let spec = ProjectSpec {
        title: "My App",
        app: app(),
        components: &components,
        libraries: vec!["user".to_string()],
    };
    let (files, warnings) = project_files(lang, &spec).expect(lang);
    (
        files.into_iter().map(|f| (f.path, f.contents)).collect(),
        warnings,
    )
}

fn file<'a>(files: &'a [(String, String)], path: &str) -> &'a str {
    files
        .iter()
        .find(|(p, _)| p == path)
        .map(|(_, c)| c.as_str())
        .unwrap_or_else(|| {
            panic!(
                "no {path} in {:?}",
                files.iter().map(|(p, _)| p).collect::<Vec<_>>()
            )
        })
}

fn has(hay: &str, needle: &str) {
    assert!(hay.contains(needle), "expected `{needle}` in:\n{hay}");
}

#[test]
fn a_rust_project_holds_the_app_its_component_libraries_and_the_named_styles() {
    let (f, _) = files("rust");
    // the app: the page calls the component, defined once in its module
    let ui = file(&f, "src/ui.rs");
    has(ui, "render_card(\"Hi\")");
    has(ui, "pub fn render_card(title: &str) -> Dom {");
    // the component library: its function and its registration
    let lib = file(&f, "src/components/user.rs");
    has(lib, "pub fn render_card(title: &str) -> Dom {");
    has(lib, "pub extern \"C\" fn register_user_library() -> ComponentLibrary {");
    has(file(&f, "src/components/mod.rs"), "pub mod user;");
    // the page's stylesheet as named styles (the CSS generator)
    has(file(&f, "src/styles.rs"), "pub fn style_note()");
    let main = file(&f, "src/main.rs");
    has(main, "mod ui;");
    has(main, "mod components;");
    has(main, "mod styles;");
    has(file(&f, "Cargo.toml"), "registry = \"azul\"");
    let readme = file(&f, "README.md");
    has(readme, "src/components/user.rs");
    has(readme, "src/styles.rs");
}

#[test]
fn a_c_project_puts_each_library_and_the_styles_in_headers_of_their_own() {
    let (f, _) = files("c");
    for path in ["main.c", "ui.h", "Makefile", "components/user.h", "styles.h", "README.md"] {
        let _ = file(&f, path);
    }
    has(file(&f, "components/user.h"), "AzComponentLibrary register_user_library(void) {");
}

#[test]
fn a_language_without_dom_export_writes_its_module_and_says_why() {
    let (f, warnings) = files("perl");
    assert!(f.iter().any(|(p, _)| p.starts_with("ui.")), "{:?}", f.iter().map(|(p, _)| p).collect::<Vec<_>>());
    assert!(
        warnings.iter().any(|w| w.contains("does not print DOM")),
        "{warnings:?}"
    );
}

#[test]
fn an_unknown_language_is_refused_with_the_list() {
    let map = map();
    let components = Components::rendered_only(&map);
    let spec = ProjectSpec {
        title: "T",
        app: app(),
        components: &components,
        libraries: Vec::new(),
    };
    let err = project_files("klingon", &spec).err().expect("refused");
    assert!(err.contains("available:"), "{err}");
}

#[test]
fn a_page_app_takes_its_body_and_every_style_block() {
    let map = map();
    let components = Components::rendered_only(&map);
    let page = vec![el(
        "html",
        &[],
        vec![
            el("head", &[], vec![el("style", &[], vec![txt(".a { margin-top: 1px; }")])]),
            el(
                "body",
                &[],
                vec![
                    el("style", &[], vec![txt(".b { margin-top: 2px; }")]),
                    el("p", &[("class", "a b")], vec![txt("x")]),
                ],
            ),
        ],
    )];
    let spec = ProjectSpec {
        title: "T",
        app: AppMarkup::Page(page),
        components: &components,
        libraries: Vec::new(),
    };
    let (files, _) = project_files("rust", &spec).expect("rust");
    let ui = &files.iter().find(|f| f.path == "src/ui.rs").expect("ui").contents;
    has(ui, "margin-top: 1px;");
    has(ui, "margin-top: 2px;");
    has(ui, "Dom::create_body()");
}

// ── HTML → DOM (code): pasted markup through the same family ──

#[test]
fn pasted_html_with_a_style_block_becomes_a_render_function_with_its_css() {
    let map = ComponentMap::with_builtin();
    let components = Components::rendered_only(&map);
    let pasted = vec![
        el("style", &[], vec![txt(".card { padding-top: 4px; }")]),
        el(
            "div",
            &[("class", "card")],
            vec![el("p", &[("style", "color: red")], vec![txt("Hello")])],
        ),
    ];
    let out = html_code(&pasted, "rust", CodeMode::Function, None, false, &components).expect("rust");
    // named after its root's class, like a subtree export
    assert_eq!(out.file_name, "render_card.rs");
    has(&out.code, "pub fn render_card() -> Dom {");
    has(&out.code, ".with_css(azul::str::String::from(\"padding-top: 4px;\"))");
    has(&out.code, ".with_css(azul::str::String::from(\"color: red\"))");
    has(&out.code, "Dom::create_p_with_text(azul::str::String::from(\"Hello\"))");
    assert!(out.files.is_empty(), "a function is one file");

    let c = html_code(&pasted, "c", CodeMode::Function, Some("build"), false, &components).expect("c");
    has(&c.code, "static AzDom build(void) {");
    let py = html_code(&pasted, "python", CodeMode::Function, None, false, &components).expect("py");
    has(&py.code, "def render_card():");
}

#[test]
fn a_pasted_document_is_its_body_and_an_app_is_a_project() {
    let map = ComponentMap::with_builtin();
    let components = Components::rendered_only(&map);
    let doc = vec![el(
        "html",
        &[],
        vec![
            el("head", &[], vec![el("style", &[], vec![txt("h1 { margin-top: 3px; }")])]),
            el("body", &[], vec![el("h1", &[], vec![txt("Title")])]),
        ],
    )];
    let f = html_code(&doc, "rust", CodeMode::Function, None, false, &components).expect("fn");
    has(&f.code, "pub fn render_ui() -> Dom {");
    has(&f.code, "Dom::create_body()");
    has(&f.code, "margin-top: 3px;");

    let app = html_code(&doc, "rust", CodeMode::App, None, false, &components).expect("app");
    assert_eq!(app.file_name, "main.rs");
    assert!(app.files.iter().any(|f| f.path == "src/ui.rs"), "{:?}", app.files);
}

#[test]
fn the_css_the_html_carries_comes_along_as_named_styles_when_asked() {
    let map = ComponentMap::with_builtin();
    let components = Components::rendered_only(&map);
    let pasted = vec![
        el("style", &[], vec![txt(".card { padding-top: 4px; }")]),
        el("div", &[("class", "card")], vec![]),
    ];
    let out = html_code(&pasted, "rust", CodeMode::Function, None, true, &components).expect("rust");
    let styles = out
        .files
        .iter()
        .find(|f| f.path == "styles.rs")
        .expect("the styles file");
    has(&styles.contents, "pub fn style_card()");
    assert!(out.files.iter().any(|f| f.path == out.file_name), "{:?}", out.files);
}

#[test]
fn a_language_without_dom_export_answers_with_its_reason() {
    let map = ComponentMap::with_builtin();
    let components = Components::rendered_only(&map);
    let out = html_code(
        &[el("p", &[], vec![txt("x")])],
        "perl",
        CodeMode::Function,
        None,
        false,
        &components,
    )
    .expect("perl");
    assert!(
        out.warnings.iter().any(|w| w.contains("does not print DOM")),
        "{:?}",
        out.warnings
    );
    assert!(!out.code.trim().is_empty());
}
