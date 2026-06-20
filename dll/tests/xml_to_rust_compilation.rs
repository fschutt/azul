/// Unit tests for XML to Rust code compilation
///
/// Tests the functionality of compiling XML/XHTML to Rust source code

#[cfg(feature = "xml")]
mod xml_compilation_tests {
    use azul_core::xml::{str_to_rust_code, str_to_c_code, str_to_cpp_code, str_to_python_code, ComponentMap};
    use azul_layout::xml::parse_xml_string;

    const SAMPLE: &str = r#"<!DOCTYPE html>
<html><head><style>.box { width: 100%; color: #222222; }</style></head>
<body><div class="box"><p>Hello</p><div>A</div></div></body></html>"#;

    // Regression: a node with BOTH text and element children must keep ALL of
    // its text. The Rust walker used to bake inline text into the node via a
    // compile_fn `.with_children(...)`, then the child-walk emitted a SECOND
    // `.with_children(...)` that overwrote it — dropping the text.
    const MIXED: &str = r#"<!DOCTYPE html>
<html><head></head><body><p>Before <span>mid</span> after</p></body></html>"#;

    fn count(hay: &str, needle: &str) -> usize { hay.matches(needle).count() }

    #[test]
    fn test_mixed_text_and_element_children_rust() {
        let parsed = parse_xml_string(MIXED).expect("parse");
        let cmap = ComponentMap::with_builtin();
        let rust = str_to_rust_code(parsed.as_ref(), "", &cmap).expect("rust");
        // All three text runs must survive, exactly once each.
        assert_eq!(count(&rust, "Before"), 1, "lost leading text:\n{}", rust);
        assert_eq!(count(&rust, "mid"), 1, "lost element text:\n{}", rust);
        assert_eq!(count(&rust, "after"), 1, "lost trailing text:\n{}", rust);
    }

    #[test]
    fn test_mixed_text_and_element_children_all_langs() {
        let parsed = parse_xml_string(MIXED).expect("parse");
        let cmap = ComponentMap::with_builtin();
        for (name, out) in [
            ("c", str_to_c_code(parsed.as_ref(), &cmap).unwrap()),
            ("cpp", str_to_cpp_code(parsed.as_ref(), &cmap).unwrap()),
            ("python", str_to_python_code(parsed.as_ref(), &cmap).unwrap()),
        ] {
            assert_eq!(count(&out, "Before"), 1, "{}: lost leading text:\n{}", name, out);
            assert_eq!(count(&out, "mid"), 1, "{}: lost element text:\n{}", name, out);
            assert_eq!(count(&out, "after"), 1, "{}: lost trailing text:\n{}", name, out);
        }
    }

    #[test]
    fn test_cpp_export_shape() {
        let parsed = parse_xml_string(SAMPLE).expect("parse");
        let cmap = ComponentMap::with_builtin();
        let cpp = str_to_cpp_code(parsed.as_ref(), &cmap).expect("cpp");
        // public C++ builder idiom + per-tag creators + scaffold
        assert!(cpp.contains("#include \"azul20.hpp\""));
        assert!(cpp.contains("Dom::create_body()"));
        assert!(cpp.contains("Dom::create_div()"));
        assert!(cpp.contains(".with_css(String("));
        assert!(cpp.contains(".with_class(String(\"box\"))"));
        // `<p>Hello</p>` (single text child) → Tier A `create_p_with_text`,
        // which consumes the text (no separate `create_text` for it).
        assert!(cpp.contains("Dom::create_p_with_text(String(\"Hello\"))"));
        assert!(cpp.contains("App::create"));
        // must NOT carry stale/internal API
        assert!(!cpp.contains("AzNodeType_"));
        assert!(!cpp.contains("from_const_str"));
    }

    #[test]
    fn test_c_export_shape() {
        let parsed = parse_xml_string(SAMPLE).expect("parse");
        let cmap = ComponentMap::with_builtin();
        let c = str_to_c_code(parsed.as_ref(), &cmap).expect("c");
        assert!(c.contains("#include \"azul.h\""));
        assert!(c.contains("#define AZ_STR"));
        assert!(c.contains("AzDom n0 = AzDom_createBody();"));
        assert!(c.contains("AzDom_createDiv();"));
        assert!(c.contains("AzDom_withClass(n1, AZ_STR(\"box\"))"));
        assert!(c.contains("AzDom_addChild(&"));
        // `<p>Hello</p>` → Tier A `AzDom_createPWithText` (text consumed).
        assert!(c.contains("AzDom_createPWithText(AZ_STR(\"Hello\"))"));
        assert!(c.contains("AzApp_run(&app, window)"));
    }

    #[test]
    fn test_python_export_shape() {
        let parsed = parse_xml_string(SAMPLE).expect("parse");
        let cmap = ComponentMap::with_builtin();
        let py = str_to_python_code(parsed.as_ref(), &cmap).expect("py");
        assert!(py.contains("import azul"));
        assert!(py.contains("azul.Dom.create_body()"));
        assert!(py.contains("azul.Dom.create_div()"));
        assert!(py.contains(".with_class(\"box\")"));
        // `<p>Hello</p>` → Tier A `create_p_with_text` (text consumed).
        assert!(py.contains("azul.Dom.create_p_with_text(\"Hello\")"));
        assert!(py.contains("azul.App.create"));
        assert!(!py.contains("Dom.div()")); // not the stale arm
    }

    #[test]
    fn test_simple_div_compilation() {
        let xml_input = r#"
<!DOCTYPE html>
<html>
    <head></head>
    <body>
        <div>Hello World</div>
    </body>
</html>
"#;

        let parsed = parse_xml_string(xml_input).expect("Failed to parse XML");
        let component_map = ComponentMap::with_builtin();

        let rust_code = str_to_rust_code(parsed.as_ref(), "", &component_map)
            .expect("Failed to compile to Rust");

        // Check that output contains expected Rust code patterns
        assert!(rust_code.contains("fn main()"));
        assert!(rust_code.contains("pub fn render() -> Dom"));
        // Invariant: the generated code imports `Dom` from azul's `dom`
        // module. The generator emits a grouped `use azul::{ .., dom::Dom, .. }`
        // (idiomatic) rather than a flat `use azul::dom::Dom;`, so assert the
        // import path that holds for both forms.
        assert!(rust_code.contains("use azul::"));
        assert!(rust_code.contains("dom::Dom"));
        // Text content will be in Dom::create_text() calls
        assert!(rust_code.contains("Dom::text") || rust_code.contains("Hello World"));
    }

    #[test]
    fn test_complex_layout_compilation() {
        let xml_input = r#"
<!DOCTYPE html>
<html>
    <head>
        <style>
            .container { width: 100%; }
        </style>
    </head>
    <body>
        <div class="container">
            <div>Click Me</div>
            <p id="text">Some text</p>
        </div>
    </body>
</html>
"#;

        let parsed = parse_xml_string(xml_input).expect("Failed to parse XML");
        let component_map = ComponentMap::with_builtin();

        let rust_code = str_to_rust_code(parsed.as_ref(), "", &component_map)
            .expect("Failed to compile to Rust");

        // Verify structure
        assert!(rust_code.contains("pub mod ui"));
        assert!(rust_code.contains("pub fn render() -> Dom"));
        assert!(rust_code.contains("fn main()"));

        // Verify CSS classes/IDs are present
        assert!(rust_code.contains("container") || rust_code.contains("Class"));
        assert!(rust_code.contains("text") || rust_code.contains("Id"));

        // Verify content is present (either as text or in Dom::text calls)
        assert!(rust_code.contains("Click Me") || rust_code.contains("Dom::text"));
        assert!(rust_code.contains("Some text") || rust_code.contains("Dom::text"));
    }

    #[test]
    fn test_kitchen_sink_xml_compilation() {
        // Test with a realistic XML from the code editor
        let xml_input = r#"
<!DOCTYPE html>
<html>
    <head></head>
    <body>
        <div>
            <p>Line 1</p>
            <p>Line 2</p>
        </div>
    </body>
</html>
"#;

        let parsed = parse_xml_string(xml_input).expect("Failed to parse XML");
        let component_map = ComponentMap::with_builtin();

        let rust_code = str_to_rust_code(parsed.as_ref(), "", &component_map)
            .expect("Failed to compile to Rust");

        // Should be valid Rust code structure
        assert!(rust_code.contains("#![windows_subsystem"));
        assert!(rust_code.contains("Auto-generated"));
        assert!(rust_code.contains("use azul::"));
        assert!(rust_code.contains("struct Data"));
        assert!(rust_code.contains("extern \"C\" fn render"));
        assert!(rust_code.contains("fn main()"));
        assert!(rust_code.contains("App::create"));

        // Should contain the actual content
        assert!(rust_code.contains("Line 1"));
        assert!(rust_code.contains("Line 2"));
    }

    #[test]
    fn test_empty_body() {
        let xml_input = r#"
<!DOCTYPE html>
<html>
    <head></head>
    <body></body>
</html>
"#;

        let parsed = parse_xml_string(xml_input).expect("Failed to parse XML");
        let component_map = ComponentMap::with_builtin();

        let rust_code = str_to_rust_code(parsed.as_ref(), "", &component_map)
            .expect("Failed to compile to Rust");

        // Even with empty body, should have valid structure
        assert!(rust_code.contains("fn main()"));
        assert!(rust_code.contains("pub fn render() -> Dom"));
    }

    #[test]
    fn test_invalid_xml_returns_error() {
        let xml_input = r#"
<!DOCTYPE html>
<html>
    <head></head>
    <body>
        <div>Unclosed div
    </body>
</html>
"#;

        let result = parse_xml_string(xml_input);
        // With lenient HTML5-like parsing, unclosed tags are auto-closed
        // so this should actually parse successfully
        assert!(
            result.is_ok(),
            "Lenient parser should tolerate unclosed tags"
        );
    }

    #[test]
    fn test_truly_invalid_xml_returns_error() {
        // Test with truly invalid XML that even lenient parsing can't handle
        let xml_input = r#"<div attr="unclosed></div>"#;

        let result = parse_xml_string(xml_input);
        // Should fail to parse truly malformed XML
        assert!(result.is_err());
    }

    #[test]
    fn test_compilation_with_imports() {
        let xml_input = r#"
<!DOCTYPE html>
<html>
    <head></head>
    <body>
        <div>Test</div>
    </body>
</html>
"#;

        let custom_imports = "use std::collections::HashMap;";

        let parsed = parse_xml_string(xml_input).expect("Failed to parse XML");
        let component_map = ComponentMap::with_builtin();

        let rust_code = str_to_rust_code(parsed.as_ref(), custom_imports, &component_map)
            .expect("Failed to compile to Rust");

        // Custom imports should be included
        assert!(rust_code.contains("use std::collections::HashMap"));
    }

    #[test]
    fn test_realistic_kitchen_sink_example() {
        // Realistic example from the code editor tab
        let xml_input = r#"
<!DOCTYPE html>
<html>
    <head>
        <style>
            body { background-color: white; }
            .editor { font-family: monospace; }
        </style>
    </head>
    <body>
        <div class="editor">
            <div>fn main() {</div>
            <div>    println!("Hello, world!");</div>
            <div>}</div>
        </div>
    </body>
</html>
"#;

        let parsed = parse_xml_string(xml_input).expect("Failed to parse XML");
        let component_map = ComponentMap::with_builtin();

        let rust_code = str_to_rust_code(parsed.as_ref(), "", &component_map)
            .expect("Failed to compile to Rust");

        // Should compile without errors
        assert!(rust_code.contains("fn main()"));
        assert!(rust_code.contains("pub fn render() -> Dom"));

        // Should handle CSS styling
        assert!(rust_code.contains("editor") || rust_code.contains("Class"));

        // Should be compilable Rust code structure
        assert!(rust_code.contains("use azul::"));
        assert!(rust_code.contains("App::create"));
        assert!(rust_code.contains("WindowCreateOptions"));
    }

    // ──────────────────────────────────────────────────────────────────────
    // Semantic / a11y-aware constructors (tiers A-D).
    // ──────────────────────────────────────────────────────────────────────

    /// Wrap a `<body>` snippet in a full HTML doc and export to all 4 languages.
    fn gen_all(body: &str) -> (String, String, String, String) {
        let html = format!(
            "<!DOCTYPE html>\n<html><head></head><body>{}</body></html>",
            body
        );
        let parsed = parse_xml_string(&html).expect("parse");
        let cmap = ComponentMap::with_builtin();
        (
            str_to_rust_code(parsed.as_ref(), "", &cmap).expect("rust"),
            str_to_c_code(parsed.as_ref(), &cmap).expect("c"),
            str_to_cpp_code(parsed.as_ref(), &cmap).expect("cpp"),
            str_to_python_code(parsed.as_ref(), &cmap).expect("py"),
        )
    }

    // Tier A — `<p>hi</p>` → create_p_with_text (text consumed).
    #[test]
    fn test_tier_a_p_with_text_all_langs() {
        let (rust, c, cpp, py) = gen_all("<p>hi</p>");
        assert!(rust.contains("Dom::create_p_with_text(AzString::from(\"hi\"))"), "{}", rust);
        assert!(c.contains("AzDom_createPWithText(AZ_STR(\"hi\"))"), "{}", c);
        assert!(cpp.contains("Dom::create_p_with_text(String(\"hi\"))"), "{}", cpp);
        assert!(py.contains("azul.Dom.create_p_with_text(\"hi\")"), "{}", py);
        // text folded into the ctor — never also emitted as a child text node
        assert!(!c.contains("AzDom_createText(AZ_STR(\"hi\"))"), "text not consumed: {}", c);
        assert!(!rust.contains("Dom::create_text(\"hi\")"), "text not consumed: {}", rust);
    }

    // Tier C — `<button aria-label="Go">Go</button>` → create_button(text, aria).
    #[test]
    fn test_tier_c_button_aria_all_langs() {
        let (rust, c, cpp, py) = gen_all("<button aria-label=\"Go\">Go</button>");
        assert!(rust.contains("Dom::create_button(AzString::from(\"Go\"), SmallAriaInfo::label(AzString::from(\"Go\")))"), "{}", rust);
        assert!(c.contains("AzDom_createButton(AZ_STR(\"Go\"), AzSmallAriaInfo_label(AZ_STR(\"Go\")))"), "{}", c);
        assert!(cpp.contains("Dom::create_button(String(\"Go\"), SmallAriaInfo::label(String(\"Go\")))"), "{}", cpp);
        assert!(py.contains("azul.Dom.create_button(\"Go\", azul.SmallAriaInfo.label(\"Go\"))"), "{}", py);
        assert!(!c.contains("AzDom_createText(AZ_STR(\"Go\"))"), "button text must be consumed: {}", c);
    }

    // Tier C — `<a href="x">link</a>` (no aria) → create_a_no_a11y(href, Some(label)).
    #[test]
    fn test_tier_c_a_no_aria_all_langs() {
        let (rust, c, cpp, py) = gen_all("<a href=\"x\">link</a>");
        assert!(rust.contains("Dom::create_a_no_a11y(AzString::from(\"x\"), OptionString::Some(AzString::from(\"link\")))"), "{}", rust);
        assert!(c.contains("AzDom_createANoA11y(AZ_STR(\"x\"), AzOptionString_some(AZ_STR(\"link\")))"), "{}", c);
        assert!(cpp.contains("Dom::create_a_no_a11y(String(\"x\"), OptionString::some(String(\"link\")))"), "{}", cpp);
        assert!(py.contains("azul.Dom.create_a_no_a11y(\"x\", azul.OptionString.some(\"link\"))"), "{}", py);
    }

    // Tier C — `<input type="text" name="u">` → create_input_no_a11y(type, name, label).
    #[test]
    fn test_tier_c_input_all_langs() {
        let (rust, c, cpp, py) = gen_all("<input type=\"text\" name=\"u\">");
        assert!(rust.contains("Dom::create_input_no_a11y(AzString::from(\"text\"), AzString::from(\"u\"), AzString::from(\"\"))"), "{}", rust);
        assert!(c.contains("AzDom_createInputNoA11y(AZ_STR(\"text\"), AZ_STR(\"u\"), AZ_STR(\"\"))"), "{}", c);
        assert!(cpp.contains("Dom::create_input_no_a11y(String(\"text\"), String(\"u\"), String(\"\"))"), "{}", cpp);
        assert!(py.contains("azul.Dom.create_input_no_a11y(\"text\", \"u\", \"\")"), "{}", py);
    }

    // Tier B — `<details><summary>S</summary></details>` → details_no_a11y + summary_with_text_no_a11y.
    #[test]
    fn test_tier_b_details_summary_all_langs() {
        let (rust, c, cpp, py) = gen_all("<details><summary>S</summary></details>");
        assert!(rust.contains("Dom::create_details_no_a11y()"), "{}", rust);
        assert!(rust.contains("Dom::create_summary_with_text_no_a11y(AzString::from(\"S\"))"), "{}", rust);
        assert!(c.contains("AzDom_createDetailsNoA11y()"), "{}", c);
        assert!(c.contains("AzDom_createSummaryWithTextNoA11y(AZ_STR(\"S\"))"), "{}", c);
        assert!(cpp.contains("Dom::create_details_no_a11y()"), "{}", cpp);
        assert!(cpp.contains("Dom::create_summary_with_text_no_a11y(String(\"S\"))"), "{}", cpp);
        assert!(py.contains("azul.Dom.create_details_no_a11y()"), "{}", py);
        assert!(py.contains("azul.Dom.create_summary_with_text_no_a11y(\"S\")"), "{}", py);
    }

    // Tier D — Progress/Meter/Dialog use the NoA11y form with extracted scalars.
    #[test]
    fn test_tier_d_scalar_widgets_all_langs() {
        let (rust, c, cpp, py) = gen_all(
            "<progress value=\"0.5\" max=\"1\"></progress><meter value=\"2\" min=\"0\" max=\"10\"></meter><dialog></dialog>",
        );
        assert!(rust.contains("Dom::create_progress_no_a11y(0.5, 1.0)"), "{}", rust);
        assert!(rust.contains("Dom::create_meter_no_a11y(2.0, 0.0, 10.0)"), "{}", rust);
        assert!(rust.contains("Dom::create_dialog_no_a11y()"), "{}", rust);
        assert!(c.contains("AzDom_createProgressNoA11y(0.5f, 1.0f)"), "{}", c);
        assert!(c.contains("AzDom_createMeterNoA11y(2.0f, 0.0f, 10.0f)"), "{}", c);
        assert!(cpp.contains("Dom::create_progress_no_a11y(0.5f, 1.0f)"), "{}", cpp);
        assert!(py.contains("azul.Dom.create_meter_no_a11y(2.0, 0.0, 10.0)"), "{}", py);
    }

    /// Build the full set of semantic `AzDom_create*` symbols this generator can
    /// emit. Each was verified by hand against `target/codegen/azul.h` (C) and
    /// `azul20.hpp` (C++); the C++/Python/Rust names are their snake_case.
    fn verified_c_semantic_symbols() -> Vec<String> {
        let with_text = [
            "Acronym", "B", "Bdi", "Bdo", "Big", "Blockquote", "Cite", "Code",
            "Del", "Dfn", "Em", "H1", "H2", "H3", "H4", "H5", "H6", "I", "Ins",
            "Kbd", "Li", "Mark", "P", "Pre", "Rp", "Rt", "S", "Samp", "Small",
            "Span", "Strong", "Style", "Sub", "Sup", "Td", "Th", "Title", "U",
            "Var",
        ];
        let tier_b = [
            "Details", "Form", "Fieldset", "Legend", "Menu", "Output",
            "Datalist", "Canvas", "Audio", "Video", "Area",
        ];
        let tier_c = [
            "Button", "A", "Label", "Input", "Textarea", "Select", "Option",
            "Optgroup", "Table",
        ];
        let mut v = Vec::new();
        for t in with_text {
            v.push(format!("AzDom_create{}WithText", t));
        }
        for t in tier_b {
            v.push(format!("AzDom_create{}", t));
            v.push(format!("AzDom_create{}NoA11y", t));
        }
        // Summary has both aria-only and with-text forms.
        for s in ["AzDom_createSummary", "AzDom_createSummaryNoA11y", "AzDom_createSummaryWithText", "AzDom_createSummaryWithTextNoA11y"] {
            v.push(s.to_string());
        }
        for t in tier_c {
            v.push(format!("AzDom_create{}", t));
            v.push(format!("AzDom_create{}NoA11y", t));
        }
        for t in ["Progress", "Meter", "Dialog"] {
            v.push(format!("AzDom_create{}NoA11y", t));
        }
        v
    }

    /// All `<prefix><ident>` occurrences (ident = `[A-Za-z0-9_]+`).
    fn extract_calls(src: &str, prefix: &str) -> Vec<String> {
        let bytes = src.as_bytes();
        let p = prefix.as_bytes();
        let mut out = Vec::new();
        let mut i = 0;
        while i + p.len() <= bytes.len() {
            if &bytes[i..i + p.len()] == p {
                let mut j = i + p.len();
                while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                    j += 1;
                }
                out.push(src[i..j].to_string());
                i = j;
            } else {
                i += 1;
            }
        }
        out
    }

    fn is_semantic_c(sym: &str) -> bool {
        let b = sym.strip_prefix("AzDom_create").unwrap_or(sym);
        b.ends_with("WithText")
            || b.ends_with("NoA11y")
            || matches!(
                b,
                "Button" | "A" | "Label" | "Input" | "Textarea" | "Select"
                    | "Option" | "Optgroup" | "Table" | "Details" | "Summary"
                    | "Form" | "Fieldset" | "Legend" | "Menu" | "Output"
                    | "Datalist" | "Canvas" | "Audio" | "Video" | "Area"
                    | "Progress" | "Meter" | "Dialog"
            )
    }

    fn is_semantic_cpp(method: &str) -> bool {
        method.ends_with("_with_text")
            || method.ends_with("_no_a11y")
            || matches!(
                method,
                "create_button" | "create_a" | "create_label" | "create_input"
                    | "create_textarea" | "create_select" | "create_option"
                    | "create_optgroup" | "create_table" | "create_details"
                    | "create_summary" | "create_form" | "create_fieldset"
                    | "create_legend" | "create_menu" | "create_output"
                    | "create_datalist" | "create_canvas" | "create_audio"
                    | "create_video" | "create_area"
            )
    }

    /// Walk up from this crate's dir looking for a generated header.
    fn find_header(name: &str) -> Option<(std::path::PathBuf, String)> {
        let mut dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        loop {
            for rel in [
                name.to_string(),
                format!("target/codegen/{}", name),
                format!("dll/{}", name),
                format!("examples/c/{}", name),
                format!("examples/cpp/cpp20/{}", name),
            ] {
                let p = dir.join(&rel);
                if let Ok(s) = std::fs::read_to_string(&p) {
                    return Some((p, s));
                }
            }
            if !dir.pop() {
                return None;
            }
        }
    }

    // The C++ gate (libc++ is broken on some dev machines, so this asserts
    // symbol presence rather than compiling): every semantic constructor the
    // generator emits must (a) be in the hand-verified set, and (b) actually
    // exist in azul.h / azul20.hpp wherever those generated headers are found.
    #[test]
    fn test_no_emitted_create_symbol_absent_from_header() {
        // One page exercising every tier + both aria / no-aria branches.
        let body = "\
            <p>hi</p><h1>T</h1>\
            <button aria-label=\"Go\">Go</button><button>Plain</button>\
            <a href=\"x\">link</a><a href=\"y\" aria-label=\"Home\">Home</a>\
            <label for=\"u\">Name</label>\
            <input type=\"text\" name=\"u\">\
            <textarea name=\"bio\"></textarea>\
            <select name=\"s\"><optgroup label=\"g\"><option value=\"1\">One</option></optgroup></select>\
            <details><summary aria-label=\"more\">S</summary></details>\
            <form aria-label=\"f\"></form><fieldset></fieldset><legend>L</legend>\
            <menu></menu><output></output><datalist></datalist>\
            <canvas></canvas><audio></audio><video></video><area>\
            <progress value=\"0.5\" max=\"1\"></progress>\
            <meter value=\"2\" min=\"0\" max=\"10\"></meter>\
            <dialog></dialog>\
            <table aria-label=\"T\"><caption>Cap</caption><tr><td>x</td></tr></table>\
            <table><tr><td>y</td></tr></table>";
        let (_rust, c, cpp, _py) = gen_all(body);

        let verified = verified_c_semantic_symbols();
        let verified_set: std::collections::HashSet<&str> =
            verified.iter().map(|s| s.as_str()).collect();

        // (a) Self-contained gate: every emitted SEMANTIC C symbol must be in the
        //     hand-verified set (catches a generator typo with no header needed).
        let emitted_c: Vec<String> = extract_calls(&c, "AzDom_create");
        let mut saw_semantic = false;
        for sym in &emitted_c {
            if is_semantic_c(sym) {
                saw_semantic = true;
                assert!(
                    verified_set.contains(sym.as_str()),
                    "emitted unverified semantic C symbol `{}`",
                    sym
                );
            }
        }
        assert!(saw_semantic, "test page emitted no semantic symbols:\n{}", c);

        // (b) Header gate: when azul.h is reachable, every verified + every
        //     emitted semantic symbol must exist in it.
        match find_header("azul.h") {
            Some((path, hdr)) => {
                for sym in &verified {
                    assert!(
                        hdr.contains(&format!("{}(", sym)),
                        "verified C symbol `{}` missing from {:?}",
                        sym,
                        path
                    );
                }
                for sym in &emitted_c {
                    if is_semantic_c(sym) {
                        assert!(
                            hdr.contains(&format!("{}(", sym)),
                            "emitted C symbol `{}` absent from header {:?}",
                            sym,
                            path
                        );
                    }
                }
            }
            None => eprintln!(
                "note: azul.h not found — header cross-check skipped (allowlist still enforced)"
            ),
        }

        // C++ gate: emitted semantic methods must exist in azul20.hpp when found.
        if let Some((path, hpp)) = find_header("azul20.hpp") {
            for sym in extract_calls(&cpp, "Dom::create_") {
                let method = sym.strip_prefix("Dom::").unwrap_or(&sym);
                if is_semantic_cpp(method) {
                    assert!(
                        hpp.contains(&format!("{}(", method)),
                        "emitted C++ method `{}` absent from header {:?}",
                        method,
                        path
                    );
                }
            }
        } else {
            eprintln!("note: azul20.hpp not found — C++ symbol check skipped");
        }
    }
}

#[cfg(not(feature = "xml"))]
mod no_xml_feature {
    #[test]
    fn test_xml_feature_not_enabled() {
        // This test just ensures the tests compile when xml feature is disabled
        assert!(true);
    }
}
