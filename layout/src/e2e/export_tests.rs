//! AzBuilder's quick exports and Export > Code, driven end to end through the
//! REAL op dispatcher on a headless window: every message the export dialogs
//! (`debugger-export.js`) send — `get_codegen_languages`, `get_css_rules`,
//! `compile_css`, `html_to_code`, `export_subtree_code`,
//! `export_component_code` — and the project behind Export > Code
//! (`export_code`, `export_code_zip`).
//!
//! Each scenario reads the op's answer from the step results (`response` is
//! the `ResponseData` JSON, `{type, value}`) and checks the generated code
//! itself, not only that an answer came back. The round trip the export has
//! to hold: a subtree converted into a component exports to code that builds
//! the same subtree again, with the converted texts / attributes as its
//! parameters (and survives a library export + import as a template).
//!
//! uid allocation as in `builder_tests.rs`: `<body>` is 0, every
//! `builder_insert` takes the next uid.

use super::{run_e2e_test, E2eTest, E2eTestResult};

fn run(name: &str, steps: Vec<serde_json::Value>) -> E2eTestResult {
    let test: E2eTest = serde_json::from_value(serde_json::json!({
        "name": name,
        "config": { "continue_on_failure": true },
        "setup": { "window_width": 400, "window_height": 300, "dpi": 96 },
        "steps": steps,
    }))
    .expect("the scenario literal is a valid E2eTest");
    run_e2e_test(&test)
}

fn failures(result: &E2eTestResult) -> String {
    result
        .steps
        .iter()
        .filter(|s| s.status != "pass")
        .map(|s| {
            format!(
                "  step {} `{}`: {}",
                s.step_index,
                s.op,
                s.error.clone().unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Step `i` passed; its answer's `value`.
fn value(result: &E2eTestResult, i: usize) -> serde_json::Value {
    let step = result
        .steps
        .iter()
        .find(|s| s.step_index == i)
        .unwrap_or_else(|| panic!("no step {i}:\n{}", failures(result)));
    assert_eq!(
        step.status,
        "pass",
        "step {i} `{}` must pass:\n{}",
        step.op,
        failures(result)
    );
    step.response
        .as_ref()
        .and_then(|r| r.get("value"))
        .cloned()
        .unwrap_or_else(|| panic!("step {i} `{}` answered no data", step.op))
}

/// Step `i` was refused; its error message.
fn refusal(result: &E2eTestResult, i: usize) -> String {
    let step = result
        .steps
        .iter()
        .find(|s| s.step_index == i)
        .unwrap_or_else(|| panic!("no step {i}"));
    assert_eq!(step.status, "fail", "step {i} `{}` must be refused", step.op);
    step.error.clone().unwrap_or_default()
}

/// The generated code in step `i`'s answer.
fn code(result: &E2eTestResult, i: usize) -> String {
    value(result, i)
        .get("code")
        .and_then(|c| c.as_str())
        .unwrap_or_else(|| panic!("step {i} has no code"))
        .to_string()
}

fn has(haystack: &str, needle: &str) {
    assert!(
        haystack.contains(needle),
        "expected `{needle}` in:\n{haystack}"
    );
}

fn has_not(haystack: &str, needle: &str) {
    assert!(
        !haystack.contains(needle),
        "`{needle}` must not be in:\n{haystack}"
    );
}

/// `<body>` > `div.card` (uid 1) > [`h2` "Hello" (2), `a[href]` "Docs" (3)]
fn card_steps() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({ "op": "builder_insert", "parent": 0, "component": "div",
                            "attrs": { "class": "card" } }),
        serde_json::json!({ "op": "builder_insert", "parent": 1, "component": "h2",
                            "attrs": { "text": "Hello" } }),
        serde_json::json!({ "op": "builder_insert", "parent": 1, "component": "a",
                            "attrs": { "href": "https://azul.rs", "text": "Docs" } }),
    ]
}

#[test]
fn the_export_dialogs_get_one_language_list_every_code_generator_and_whether_it_does_dom() {
    let result = run(
        "export_languages",
        vec![serde_json::json!({ "op": "get_codegen_languages" })],
    );
    let v = value(&result, 0);
    let langs = v["languages"].as_array().expect("one list").clone();
    let ids: Vec<&str> = langs.iter().filter_map(|l| l["id"].as_str()).collect();
    let supported = azul_css::codegen::supported_languages();
    assert_eq!(ids.join(", "), supported, "exactly azul_css::codegen::all_backends()");
    let dom: Vec<&str> = langs
        .iter()
        .filter(|l| l["dom"] == true)
        .filter_map(|l| l["id"].as_str())
        .collect();
    let printers: Vec<&str> = azul_css::codegen::all_backends()
        .iter()
        .filter(|b| b.exports_dom())
        .map(|b| b.lang())
        .collect();
    assert_eq!(dom, printers, "`dom` is each printer's exports_dom()");
    for lang in ["rust", "c", "cpp", "python", "java", "go", "swift"] {
        assert!(dom.contains(&lang), "{lang} exports a DOM: {dom:?}");
    }
    assert!(langs
        .iter()
        .all(|l| l["label"].is_string() && l["ext"].is_string() && l["dom"].is_boolean()));
    assert!(langs.iter().any(|l| l["id"] == "cpp" && l["label"] == "C++"));
}

#[test]
fn css_rules_are_listed_in_source_order_and_a_picked_subset_compiles() {
    let css = ".a { color: red; } .b { margin-top: 2px; } #c { padding-left: 1px; }";
    let result = run(
        "export_css_rules",
        vec![
            /* 0 */ serde_json::json!({ "op": "get_css_rules", "css": css }),
            /* 1 */
            serde_json::json!({ "op": "compile_css", "language": "rust", "css": css,
                                "rules": [1] }),
            /* 2 */ serde_json::json!({ "op": "compile_css", "language": "rust", "css": css }),
            /* 3 */
            serde_json::json!({ "op": "compile_css", "language": "rust", "css": css,
                                "rules": [7] }),
            /* 4 */
            serde_json::json!({ "op": "compile_css", "language": "klingon", "css": css }),
            /* 5: every code generator compiles CSS, not only the DOM ones */
            serde_json::json!({ "op": "compile_css", "language": "cobol", "css": css }),
        ],
    );
    let rules = value(&result, 0)["rules"].as_array().cloned().expect("rules");
    let selectors: Vec<&str> = rules.iter().filter_map(|r| r["selector"].as_str()).collect();
    assert_eq!(selectors, [".a", ".b", "#c"]);
    assert_eq!(rules[0]["classes"], serde_json::json!(["a"]));
    assert!(rules[1]["declarations"]
        .as_str()
        .is_some_and(|d| d.contains("margin-top")));

    let picked = value(&result, 1);
    assert_eq!(picked["rule_count"], 1);
    assert_eq!(picked["language"], "rust");
    assert!(picked["code"].as_str().is_some_and(|c| !c.trim().is_empty()));
    assert!(picked["file_name"].as_str().is_some_and(|f| f.ends_with(".rs")));
    assert_eq!(value(&result, 2)["rule_count"], 3);

    has(&refusal(&result, 3), "rule 7");
    let unknown = refusal(&result, 4);
    has(&unknown, "no code generator for \"klingon\"");
    has(&unknown, &azul_css::codegen::supported_languages());
    let cobol = value(&result, 5);
    assert_eq!(cobol["language"], "cobol");
    assert_eq!(cobol["rule_count"], 3);
}

#[test]
fn the_selected_nodes_style_is_the_rules_that_apply_to_it_and_its_style_attribute() {
    let result = run(
        "export_node_css",
        vec![
            /* 0: uid 1 */
            serde_json::json!({ "op": "builder_insert", "parent": 0, "component": "span",
                                "attrs": { "class": "pill", "text": "new" } }),
            /* 1: uid 1 becomes an instance of ui:pill */
            serde_json::json!({ "op": "builder_convert_to_component", "node": 1,
                                "library": "ui", "name": "pill" }),
            /* 2: its CSS is now part of the document's stylesheet */
            serde_json::json!({ "op": "update_component", "library": "ui", "name": "pill",
                                "css": ".pill { margin-top: 3px; } .card { padding-top: 4px; }" }),
            /* 3: uid 2 */
            serde_json::json!({ "op": "builder_insert", "parent": 0, "component": "div",
                                "attrs": { "class": "card", "style": "color: red" } }),
            /* 4 */ serde_json::json!({ "op": "get_css_rules", "source": "node", "node": 2 }),
            /* 5 */ serde_json::json!({ "op": "get_css_rules", "source": "document" }),
            /* 6 */
            serde_json::json!({ "op": "get_css_rules", "source": "component",
                                "library": "ui", "name": "pill" }),
            /* 7 */ serde_json::json!({ "op": "get_css_rules", "source": "node", "node": 1 }),
            /* 8 */
            serde_json::json!({ "op": "compile_css", "language": "rust", "source": "node",
                                "node": 2 }),
            /* 9 */ serde_json::json!({ "op": "get_css_rules", "source": "node", "node": 99 }),
        ],
    );
    let node = value(&result, 4);
    let css = node["css"].as_str().expect("css");
    has(css, "padding-top");
    has(css, ".card { color: red }");
    has_not(css, "margin-top");
    assert_eq!(node["rules"].as_array().map(Vec::len), Some(2));

    assert_eq!(value(&result, 5)["rules"].as_array().map(Vec::len), Some(2));
    assert_eq!(value(&result, 6)["rules"].as_array().map(Vec::len), Some(2));
    has(value(&result, 7)["css"].as_str().expect("css"), "margin-top");
    assert_eq!(value(&result, 8)["rule_count"], 2);
    has(&refusal(&result, 9), "99");
}

#[test]
fn a_document_subtree_exports_as_one_render_function_per_language() {
    let mut steps = card_steps();
    steps.extend([
        /* 3 */
        serde_json::json!({ "op": "export_subtree_code", "node": 1, "language": "rust" }),
        /* 4 */ serde_json::json!({ "op": "export_subtree_code", "node": 1, "language": "c" }),
        /* 5 */ serde_json::json!({ "op": "export_subtree_code", "node": 1, "language": "cpp" }),
        /* 6 */
        serde_json::json!({ "op": "export_subtree_code", "node": 1, "language": "python" }),
        /* 7 */
        serde_json::json!({ "op": "export_subtree_code", "node": 0, "language": "rust",
                            "mode": "app" }),
        /* 8 */
        serde_json::json!({ "op": "export_subtree_code", "node": 1, "language": "rust",
                            "function_name": "build_card" }),
        /* 9 */ serde_json::json!({ "op": "export_subtree_code", "node": 99, "language": "rust" }),
        /* 10 */
        serde_json::json!({ "op": "export_subtree_code", "node": 1, "language": "klingon" }),
        /* 11: a printer that builds the DOM through its binding's wrapper classes */
        serde_json::json!({ "op": "export_subtree_code", "node": 1, "language": "java" }),
    ]);
    let result = run("export_subtree", steps);

    // The same printers as the CSS export (azul_css::codegen), Rust:
    let rust = code(&result, 3);
    has(&rust, "pub fn render_card() -> Dom {");
    has(&rust, ".with_class(azul::str::String::from(\"card\"))");
    has(
        &rust,
        "Dom::create_h2_with_text(azul::str::String::from(\"Hello\"))",
    );
    // A link's text is its accessible name.
    has(
        &rust,
        "Dom::create_a(azul::str::String::from(\"https://azul.rs\"), \
         azul::str::String::from(\"Docs\"), \
         SmallAriaInfo::label(azul::str::String::from(\"Docs\")))",
    );
    // The builder's own marker classes stay in the builder.
    has_not(&rust, "azb-");
    assert_eq!(value(&result, 3)["file_name"], "render_card.rs");

    let c = code(&result, 4);
    has(&c, "static AzDom render_card(void) {");
    has(
        &c,
        "AzDom_createH2WithText(AzString_copyFromBytes((const uint8_t*)\"Hello\", 0, 5))",
    );
    has_not(&c, "azb-");
    assert_eq!(value(&result, 4)["file_name"], "render_card.h");
    has(&code(&result, 5), "inline AzDom render_card() {");
    has(&code(&result, 6), "def render_card():");

    // An app is a project: the dialog shows its main file, `files` has all.
    let app = value(&result, 7);
    assert_eq!(app["file_name"], "main.rs");
    let main = app["code"].as_str().expect("code");
    has(main, "fn main()");
    has(main, "WindowCreateOptions::create(layout)");
    has(main, "ui::render_ui()");
    let files = app["files"].as_array().expect("files");
    let file = |path: &str| {
        files
            .iter()
            .find(|f| f["path"] == path)
            .and_then(|f| f["contents"].as_str())
            .unwrap_or_else(|| panic!("{path} in {files:?}"))
            .to_string()
    };
    has(&file("src/ui.rs"), "Dom::create_body()");
    has(&file("Cargo.toml"), "registry = \"azul\"");

    has(&code(&result, 8), "pub fn build_card() -> Dom {");
    has(&refusal(&result, 9), "99");
    has(&refusal(&result, 10), "no code generator for \"klingon\"");

    let java = value(&result, 11);
    assert_eq!(java["file_name"], "render_card.java");
    let java_code = java["code"].as_str().expect("code");
    has(java_code, "public static Dom renderCard() {");
    has(java_code, ".withClass(\"card\")");
    has(java_code, "Dom.createH2WithText(\"Hello\")");
    assert!(
        java["warnings"].as_array().is_some_and(Vec::is_empty),
        "{java}"
    );
}

#[test]
fn a_converted_component_exports_to_code_that_recreates_its_subtree() {
    let mut steps = card_steps();
    steps.extend([
        /* 3: the div (uid 1) becomes user:my-card: text → `text`, href → `href`,
         * "Docs" → `text_2` */
        serde_json::json!({ "op": "builder_convert_to_component", "node": 1,
                            "library": "user", "name": "my-card" }),
        /* 4 */
        serde_json::json!({ "op": "export_component_code", "library": "user",
                            "name": "my-card", "language": "rust" }),
        /* 5 */
        serde_json::json!({ "op": "export_component_code", "library": "user",
                            "name": "my-card", "language": "c" }),
        /* 6 */
        serde_json::json!({ "op": "export_component_code", "library": "user",
                            "name": "my-card", "language": "cpp" }),
        /* 7 */
        serde_json::json!({ "op": "export_component_code", "library": "user",
                            "name": "my-card", "language": "python" }),
        /* 8: the Components view's "compile_fn" is the component as code (the
         * same printers as export_component_code) */
        serde_json::json!({ "op": "get_component_source", "library": "user",
                            "name": "my-card", "source_type": "compile_fn",
                            "language": "rust" }),
        /* 9: the subtree export of the document CALLS the card's function
         * (component boundaries stay calls) and defines it once */
        serde_json::json!({ "op": "export_subtree_code", "node": 0, "language": "rust" }),
        /* 10 */
        serde_json::json!({ "op": "export_component_code", "library": "user",
                            "name": "nope", "language": "rust" }),
        /* 11: the same in C: the card's function comes before its caller */
        serde_json::json!({ "op": "export_subtree_code", "node": 0, "language": "c" }),
    ]);
    let result = run("export_converted_component", steps);

    let rust = code(&result, 4);
    // The render function rebuilds the subtree, its texts / attributes are
    // the parameters…
    has(
        &rust,
        "pub fn render_my_card(text: &str, href: &str, text_2: &str) -> Dom {",
    );
    has(&rust, ".with_class(azul::str::String::from(\"card\"))");
    has(
        &rust,
        "Dom::create_h2_with_text(azul::str::String::from(text))",
    );
    has(
        &rust,
        "Dom::create_a(azul::str::String::from(href), azul::str::String::from(text_2), \
         SmallAriaInfo::label(azul::str::String::from(text_2)))",
    );
    // …the defaults are what was converted…
    has(&rust, "render_my_card(\"Hello\", \"https://azul.rs\", \"Docs\")");
    // …and the component registers again.
    has(
        &rust,
        "pub extern \"C\" fn register_user_library() -> ComponentLibrary {",
    );
    has(
        &rust,
        "id: ComponentId::create(azul::str::String::from(\"user\"), \
         azul::str::String::from(\"my-card\")),",
    );
    has(&rust, "string_field(\"text\", \"Hello\",");
    has(&rust, "let arg_text = model_string(model, \"text\", \"Hello\");");
    assert_eq!(value(&result, 4)["file_name"], "user_my_card.rs");

    let c = code(&result, 5);
    has(
        &c,
        "static AzDom render_my_card(const char* text, const char* href, const char* text_2) {",
    );
    has(
        &c,
        "AzDom_createH2WithText(AzString_copyFromBytes((const uint8_t*)text, 0, strlen(text)))",
    );
    has(&c, "AzComponentLibrary register_user_library(void) {");
    has(&c, "char* arg_text = az_model_string(model, \"text\", \"Hello\");");
    assert_eq!(value(&result, 5)["file_name"], "user_my_card.h");

    let cpp = code(&result, 6);
    has(
        &cpp,
        "inline AzDom render_my_card(const std::string& text, const std::string& href, const \
         std::string& text_2) {",
    );
    has(&cpp, "AzDom_createH2WithText(az_string(text))");
    has(&cpp, "AzComponentLibrary register_user_library(void) {");

    has(
        &code(&result, 7),
        "def render_my_card(text=\"Hello\", href=\"https://azul.rs\", text_2=\"Docs\"):",
    );

    let source = value(&result, 8)["source"]
        .as_str()
        .expect("source")
        .to_string();
    has(&source, "pub fn render_my_card(text: &str, href: &str, text_2: &str) -> Dom {");
    has_not(&source, "children.push");

    let doc = code(&result, 9);
    // The instance is a call with the converted values …
    has(&doc, "render_my_card(\"Hello\", \"https://azul.rs\", \"Docs\")");
    // … of the card's function, defined once from its template.
    has(
        &doc,
        "pub fn render_my_card(text: &str, href: &str, text_2: &str) -> Dom {",
    );
    assert_eq!(doc.matches("create_h2_with_text").count(), 1, "{doc}");
    has_not(&doc, "azb-");

    has(&refusal(&result, 10), "nope");

    let c_doc = code(&result, 11);
    let def_at = c_doc
        .find("static AzDom render_my_card(")
        .expect("the card's function");
    let call_at = c_doc
        .find("render_my_card(\"Hello\"")
        .expect("the call");
    assert!(def_at < call_at, "defined before its use:\n{c_doc}");
}

#[test]
fn a_converted_component_survives_a_library_export_and_an_import_as_a_template() {
    let mut steps = card_steps();
    steps.extend([
        /* 3 */
        serde_json::json!({ "op": "builder_convert_to_component", "node": 1,
                            "library": "user", "name": "my-card" }),
        /* 4 */ serde_json::json!({ "op": "export_component_library", "library": "user" }),
        /* 5: what a file written by step 4 looks like, imported under another name */
        serde_json::json!({ "op": "import_component_library", "library": {
            "name": "imp", "version": "1.0", "description": "",
            "components": [{
                "name": "card", "display_name": "Card",
                "fields": [{ "name": "text", "type": "String", "default": "Imported" }],
                "css": "",
                "template": "<div class=\"card\"><p>{text}</p></div>"
            }]
        } }),
        /* 6 */
        serde_json::json!({ "op": "export_component_code", "library": "imp", "name": "card",
                            "language": "rust" }),
        /* 7: it renders as its template, not as a div of its default texts */
        serde_json::json!({ "op": "builder_insert", "parent": 0, "library": "imp",
                            "component": "card" }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "wait", "ms": 100 }),
        /* 10 */ serde_json::json!({ "op": "assert_exists", "selector": ".card > p" }),
        /* 11 */ serde_json::json!({ "op": "assert_dom", "contains": "Imported" }),
    ]);
    let result = run("export_library_round_trip", steps);

    let exported = value(&result, 4);
    let comp = &exported["components"][0];
    assert_eq!(comp["name"], "my-card");
    let template = comp["template"].as_str().expect("the template is exported");
    has(template, "{text}");
    has(template, "href=\"{href}\"");

    let rust = code(&result, 6);
    has(&rust, "pub fn render_card(text: &str) -> Dom {");
    has(&rust, "Dom::create_p_with_text(azul::str::String::from(text))");
    for i in [10, 11] {
        assert!(
            result
                .steps
                .iter()
                .any(|s| s.step_index == i && s.status == "pass"),
            "step {i}:\n{}",
            failures(&result)
        );
    }
}

#[test]
fn export_code_writes_a_project_with_the_document_its_components_and_a_build_file() {
    let mut steps = card_steps();
    steps.extend([
        /* 3 */
        serde_json::json!({ "op": "builder_convert_to_component", "node": 1,
                            "library": "user", "name": "my-card" }),
        /* 4 */ serde_json::json!({ "op": "export_code", "language": "rust" }),
        /* 5 */ serde_json::json!({ "op": "export_code_zip", "language": "c" }),
        /* 6 */
        serde_json::json!({ "op": "export_code_zip", "language": "rust",
                            "library": "nothing-by-that-name" }),
    ]);
    let result = run("export_project", steps);

    let files = value(&result, 4)["files"].clone();
    let main = files["src/main.rs"].as_str().expect("src/main.rs").to_string();
    let ui = files["src/ui.rs"].as_str().expect("src/ui.rs").to_string();
    // The builder document is the app (its instance a call of the card's
    // function), not the live DOM with the builder's marker classes.
    has(&ui, "render_my_card(\"Hello\", \"https://azul.rs\", \"Docs\")");
    has(&ui, "Dom::create_h2_with_text(azul::str::String::from(text))");
    has_not(&ui, "azb-");
    has(&main, "mod ui;");
    has(&main, "mod components;");
    has(
        files["src/components/user.rs"].as_str().expect("the library"),
        "pub fn render_my_card(",
    );
    has(
        files["src/components/mod.rs"].as_str().expect("mod.rs"),
        "pub mod user;",
    );
    let cargo = files["Cargo.toml"].as_str().expect("Cargo.toml");
    has_not(cargo, "azul = \"0.0.1\"");
    has(cargo, "registry = \"azul\"");
    assert!(files[".cargo/config.toml"].is_string());
    assert!(files["README.md"].is_string());

    let zip = value(&result, 5);
    assert!(zip["download_url"]
        .as_str()
        .is_some_and(|u| u.starts_with("data:application/zip;base64,")));
    let paths: Vec<&str> = zip["files"]
        .as_array()
        .expect("files")
        .iter()
        .filter_map(|p| p.as_str())
        .collect();
    for want in ["main.c", "ui.h", "Makefile", "components/user.h", "README.md"] {
        assert!(paths.contains(&want), "{want} in {paths:?}");
    }
    // A library filter that matches nothing leaves only the app.
    let listing = value(&result, 6);
    let only_app: Vec<&str> = listing["files"]
        .as_array()
        .expect("files")
        .iter()
        .filter_map(|p| p.as_str())
        .collect();
    assert!(!only_app.iter().any(|p| p.contains("components/")), "{only_app:?}");
}

// ── HTML → DOM (code): pasted markup, `html_to_code` ──

const PASTED: &str = "<style>.card { padding-top: 4px; }</style>\n\
                      <div class=\"card\">\n  <h2>Hello</h2>\n  \
                      <a href=\"https://azul.rs\">Docs</a>\n</div>";

#[test]
fn pasted_html_becomes_a_render_function_in_rust_c_and_python() {
    let result = run(
        "html_to_code_function",
        vec![
            /* 0 */ serde_json::json!({ "op": "html_to_code", "html": PASTED, "language": "rust" }),
            /* 1 */ serde_json::json!({ "op": "html_to_code", "html": PASTED, "language": "c" }),
            /* 2 */
            serde_json::json!({ "op": "html_to_code", "html": PASTED, "language": "python" }),
            /* 3 */
            serde_json::json!({ "op": "html_to_code", "html": PASTED, "language": "rust",
                                "function_name": "build_card" }),
            /* 4 */
            serde_json::json!({ "op": "html_to_code", "html": PASTED, "language": "klingon" }),
        ],
    );
    let rust = value(&result, 0);
    assert_eq!(rust["file_name"], "render_card.rs");
    assert!(rust["errors"].as_array().is_some_and(Vec::is_empty), "{rust}");
    let rs = rust["code"].as_str().expect("code");
    has(rs, "pub fn render_card() -> Dom {");
    // The `<style>` block became the node's `with_css`.
    has(rs, ".with_css(azul::str::String::from(\"padding-top: 4px;\"))");
    has(rs, "Dom::create_h2_with_text(azul::str::String::from(\"Hello\"))");
    has(
        rs,
        "Dom::create_a(azul::str::String::from(\"https://azul.rs\"), \
         azul::str::String::from(\"Docs\"), \
         SmallAriaInfo::label(azul::str::String::from(\"Docs\")))",
    );

    let c = code(&result, 1);
    has(&c, "static AzDom render_card(void) {");
    has(
        &c,
        "AzDom_createH2WithText(AzString_copyFromBytes((const uint8_t*)\"Hello\", 0, 5))",
    );
    has(&code(&result, 2), "def render_card():");
    has(&code(&result, 3), "pub fn build_card() -> Dom {");
    has(&refusal(&result, 4), "no code generator for \"klingon\"");
}

#[test]
fn a_pasted_document_is_its_body_and_an_app_is_a_project_with_its_styles() {
    let doc = "<!DOCTYPE html>\n<html><head><style>h1 { margin-top: 3px; }</style></head>\n\
               <body><h1>Title</h1></body></html>";
    let result = run(
        "html_to_code_app",
        vec![
            /* 0 */ serde_json::json!({ "op": "html_to_code", "html": doc, "language": "rust" }),
            /* 1 */
            serde_json::json!({ "op": "html_to_code", "html": doc, "language": "rust",
                                "mode": "app", "css": true }),
        ],
    );
    let f = code(&result, 0);
    has(&f, "pub fn render_ui() -> Dom {");
    has(&f, "Dom::create_body()");
    has(&f, "margin-top: 3px;");

    let app = value(&result, 1);
    assert_eq!(app["file_name"], "main.rs");
    let paths: Vec<&str> = app["files"]
        .as_array()
        .expect("files")
        .iter()
        .filter_map(|f| f["path"].as_str())
        .collect();
    for want in ["Cargo.toml", "src/ui.rs", "src/main.rs", "src/styles.rs"] {
        assert!(paths.contains(&want), "{want} in {paths:?}");
    }
    let main = app["files"]
        .as_array()
        .and_then(|fs| fs.iter().find(|f| f["path"] == "src/main.rs"))
        .and_then(|f| f["contents"].as_str())
        .expect("main.rs");
    has(main, "mod styles;");
}

#[test]
fn a_parse_error_comes_back_with_the_line_and_column_of_the_pasted_text() {
    // Two blank lines and a doctype before the broken tag: the position is in
    // the text as pasted, not in what the parser kept of it.
    let broken = "\n\n<!DOCTYPE html>\n<p class=big>x</p>";
    let result = run(
        "html_to_code_parse_error",
        vec![serde_json::json!({ "op": "html_to_code", "html": broken, "language": "rust" })],
    );
    let v = value(&result, 0);
    let errors = v["errors"].as_array().expect("structured errors");
    assert_eq!(errors.len(), 1, "{v}");
    assert_eq!(errors[0]["line"], 4, "{v}");
    assert!(errors[0]["column"].as_u64().is_some_and(|c| c > 1), "{v}");
    assert!(
        errors[0]["message"].as_str().is_some_and(|m| !m.is_empty()),
        "{v}"
    );
    assert_eq!(v["code"], "", "no code for markup that does not parse");
}

#[test]
fn a_language_without_dom_export_answers_with_its_reason() {
    let result = run(
        "html_to_code_no_dom",
        vec![serde_json::json!({ "op": "html_to_code", "html": "<p>x</p>", "language": "perl" })],
    );
    let v = value(&result, 0);
    assert!(
        v["warnings"]
            .as_array()
            .is_some_and(|w| w.iter().any(|w| w.as_str().is_some_and(|s| s.contains("does not print DOM")))),
        "{v}"
    );
}

#[test]
fn a_component_instance_in_pasted_html_is_a_call_of_the_apps_component() {
    let mut steps = card_steps();
    steps.extend([
        /* 3 */
        serde_json::json!({ "op": "builder_convert_to_component", "node": 1,
                            "library": "user", "name": "my-card" }),
        /* 4 */
        serde_json::json!({ "op": "html_to_code", "language": "rust",
                            "html": "<user:my-card text=\"Hi\" href=\"/x\" text_2=\"Go\"/>" }),
    ]);
    let result = run("html_to_code_component", steps);
    let rust = code(&result, 4);
    has(&rust, "render_my_card(\"Hi\", \"/x\", \"Go\")");
    has(&rust, "pub fn render_my_card(text: &str, href: &str, text_2: &str) -> Dom {");
}

// ── B5: the document's own stylesheet in the exports ──

#[test]
fn the_documents_own_stylesheet_is_the_exported_apps_stylesheet_and_the_documents_css() {
    let result = run(
        "export_document_stylesheet",
        vec![
            /* 0 */
            serde_json::json!({ "op": "builder_insert", "parent": 0, "component": "p",
                                "attrs": { "class": "note", "text": "Hi" } }),
            /* 1 */
            serde_json::json!({ "op": "builder_set_stylesheet",
                                "css": ".note { margin-top: 7px; }" }),
            /* 2 */ serde_json::json!({ "op": "export_code", "language": "rust" }),
            /* 3 */ serde_json::json!({ "op": "get_css_rules", "source": "document" }),
            /* 4 */ serde_json::json!({ "op": "get_css_rules", "source": "node", "node": 1 }),
        ],
    );
    let files = value(&result, 2)["files"].clone();
    // Export > Code: the app's stylesheet as named styles (B6's project API)...
    let styles = files["src/styles.rs"]
        .as_str()
        .unwrap_or_else(|| panic!("an app with a stylesheet has src/styles.rs: {files}"));
    has(styles, "pub fn style_note() -> CssPropertyWithConditionsVec");
    has(styles, "PixelValue::px(7.0)");
    has(files["src/main.rs"].as_str().expect("src/main.rs"), "mod styles;");
    // ...and the rule reaches the node it matches.
    has(files["src/ui.rs"].as_str().expect("src/ui.rs"), "margin-top: 7px;");
    // "Compile CSS to…" on the document, and the selected node's style.
    has(value(&result, 3)["css"].as_str().expect("css"), ".note { margin-top: 7px; }");
    has(value(&result, 4)["css"].as_str().expect("css"), "margin-top");
}
