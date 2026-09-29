//! AzBuilder's quick exports and Export > Code, driven end to end through the
//! REAL op dispatcher on a headless window: every message the export dialogs
//! (`debugger-export.js`) send — `get_codegen_languages`, `get_css_rules`,
//! `compile_css`, `export_subtree_code`, `export_component_code` — and the
//! project behind Export > Code (`export_code`, `export_code_zip`).
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
    assert_eq!(dom, ["rust", "c", "cpp", "python"]);
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
        /* 11: a printer without DOM export answers, and says why it prints no UI */
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
    has(java["code"].as_str().expect("code"), "not implemented");
    assert!(
        java["warnings"]
            .as_array()
            .is_some_and(|w| w.iter().any(|w| w
                .as_str()
                .is_some_and(|w| w.contains("does not print DOM")))),
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
        /* 8: the Components view's "compile_fn" is the template as code now */
        serde_json::json!({ "op": "get_component_source", "library": "user",
                            "name": "my-card", "source_type": "compile_fn",
                            "language": "rust" }),
        /* 9: the subtree export of the document still shows the card (the
         * instance is expanded) */
        serde_json::json!({ "op": "export_subtree_code", "node": 0, "language": "rust" }),
        /* 10 */
        serde_json::json!({ "op": "export_component_code", "library": "user",
                            "name": "nope", "language": "rust" }),
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
    has(&doc, "Dom::create_h2_with_text(azul::str::String::from(\"Hello\"))");
    has_not(&doc, "azb-");

    has(&refusal(&result, 10), "nope");
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
    // The builder document is the app (its instance expanded), not the live
    // DOM with the builder's marker classes.
    has(&ui, "Dom::create_h2_with_text(azul::str::String::from(\"Hello\"))");
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
    let only_app: Vec<&str> = value(&result, 6)["files"]
        .as_array()
        .expect("files")
        .iter()
        .filter_map(|p| p.as_str())
        .collect();
    assert!(!only_app.iter().any(|p| p.contains("components/")), "{only_app:?}");
}
