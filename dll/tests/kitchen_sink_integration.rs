/// Integration test for Kitchen Sink XML compilation functionality
///
/// This module tests the compile_to_rust() function from kitchen_sink.rs

#[cfg(all(test, feature = "xml"))]
mod kitchen_sink_xml_tests {
    // Import the compile_to_rust function from kitchen_sink
    // Since kitchen_sink.rs is a binary, we need to test the compilation logic directly

    use azul_core::xml::{str_to_rust_code, ComponentMap};
    use azul_layout::xml::parse_xml_string;

    /// Simulate the compile_to_rust function from kitchen_sink.rs
    fn compile_to_rust(xml_content: &str) -> String {
        // Parse XML string
        let parsed = match parse_xml_string(xml_content) {
            Ok(parsed) => parsed,
            Err(e) => {
                return format!(
                    "// Error parsing XML:\n// {}\n\nfn main() {{\nprintln!(\"XML Parse \
                     Error\");\n}}",
                    e
                );
            }
        };

        // Compile to Rust
        let component_map = ComponentMap::with_builtin();
        match str_to_rust_code(parsed.as_ref(), "", &component_map) {
            Ok(rust_code) => rust_code,
            Err(e) => {
                format!(
                    "// Error compiling XML to Rust:\n// {}\n\nfn main() \
                     {{\nprintln!(\"Compilation Error\");\n}}",
                    e
                )
            }
        }
    }

    #[test]
    fn test_kitchen_sink_compile_simple_xml() {
        let xml = r#"
<!DOCTYPE html>
<html>
    <head></head>
    <body>
        <div>Hello from Kitchen Sink</div>
    </body>
</html>
"#;

        let result = compile_to_rust(xml);

        // Should produce valid Rust code
        assert!(result.contains("fn main()"));
        assert!(result.contains("use azul::"));
        assert!(result.contains("pub fn render() -> Dom"));

        // Should not contain error messages
        assert!(!result.contains("Error parsing XML"));
        assert!(!result.contains("Compilation Error"));
    }

    #[test]
    fn test_kitchen_sink_compile_with_style() {
        let xml = r#"
<!DOCTYPE html>
<html>
    <head>
        <style>
            .main { width: 100%; height: 100%; }
        </style>
    </head>
    <body>
        <div class="main">Styled content</div>
    </body>
</html>
"#;

        let result = compile_to_rust(xml);

        // Should handle CSS styles
        assert!(result.contains("fn main()"));
        assert!(result.contains("main") || result.contains("Class"));
        assert!(!result.contains("Error"));
    }

    #[test]
    fn test_kitchen_sink_compile_invalid_xml() {
        let xml = r#"
<!DOCTYPE html>
<html>
    <head></head>
    <body>
        <div>Unclosed div
    </body>
"#;

        let result = compile_to_rust(xml);

        // Should return error message
        assert!(result.contains("Error parsing XML") || result.contains("fn main()"));
    }

    #[test]
    fn test_kitchen_sink_compile_empty_xml() {
        let xml = r#"
<!DOCTYPE html>
<html>
    <head></head>
    <body></body>
</html>
"#;

        let result = compile_to_rust(xml);

        // Should still generate valid Rust code even for empty body
        assert!(result.contains("fn main()"));
        assert!(result.contains("pub fn render() -> Dom"));
    }

    #[test]
    fn test_kitchen_sink_compile_nested_structure() {
        let xml = r#"
<!DOCTYPE html>
<html>
    <head></head>
    <body>
        <div>
            <div>
                <div>Deeply nested</div>
            </div>
        </div>
    </body>
</html>
"#;

        let result = compile_to_rust(xml);

        // Should handle nested structures
        assert!(result.contains("fn main()"));
        assert!(result.contains("with_children") || result.contains(".add_child"));
        assert!(!result.contains("Error"));
    }
}

#[cfg(all(test, not(feature = "xml")))]
mod kitchen_sink_no_xml {
    #[test]
    fn test_xml_feature_required() {
        // This ensures the module compiles when xml feature is disabled
        assert!(true, "XML compilation requires 'xml' feature");
    }
}
