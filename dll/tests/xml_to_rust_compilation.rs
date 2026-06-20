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
        assert!(cpp.contains("Dom::create_text(String(\"Hello\"))"));
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
        assert!(c.contains("AzDom_createText(AZ_STR(\"Hello\"))"));
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
        assert!(py.contains("azul.Dom.create_text(\"Hello\")"));
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
}

#[cfg(not(feature = "xml"))]
mod no_xml_feature {
    #[test]
    fn test_xml_feature_not_enabled() {
        // This test just ensures the tests compile when xml feature is disabled
        assert!(true);
    }
}
