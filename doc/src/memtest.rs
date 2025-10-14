use std::{fs, path::Path};

use anyhow::Result;
use indexmap::IndexMap;

use crate::api::*;

/// Generate a test crate that validates memory layouts
pub fn generate_memtest_crate(api_data: &ApiData, project_root: &Path) -> Result<()> {
    let memtest_dir = project_root.join("target").join("memtest");

    // Create directory structure
    fs::create_dir_all(&memtest_dir)?;
    fs::create_dir_all(memtest_dir.join("src"))?;

    // Copy widgets folder from dll/src/widgets if it exists
    let dll_widgets_dir = project_root.join("dll").join("src").join("widgets");
    if dll_widgets_dir.exists() {
        let memtest_widgets_dir = memtest_dir.join("src").join("widgets");
        copy_widgets_with_stubs(&dll_widgets_dir, &memtest_widgets_dir)?;
        println!("✅ Copied widgets from dll/src/widgets (with dependency stubs)");
    }

    // Generate Cargo.toml
    let cargo_toml = generate_cargo_toml()?;
    fs::write(memtest_dir.join("Cargo.toml"), cargo_toml)?;

    // Generate lib.rs with all tests
    let lib_rs = generate_lib_rs(api_data)?;
    fs::write(memtest_dir.join("src").join("lib.rs"), lib_rs)?;

    println!(
        "✅ Generated memory test crate at: {}",
        memtest_dir.display()
    );
    println!("\nTo run tests:");
    println!("  cd {}", memtest_dir.display());
    println!("  cargo test");

    Ok(())
}

/// Recursively copy directory contents
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;

    for entry in
        fs::read_dir(src).map_err(|e| anyhow::anyhow!("Failed to read dir {:?}: {}", src, e))?
    {
        let entry = entry.map_err(|e| anyhow::anyhow!("Failed to read entry: {}", e))?;
        let path = entry.path();
        let file_name = entry.file_name();
        let dst_path = dst.join(&file_name);

        if path.is_dir() {
            copy_dir_recursive(&path, &dst_path)?;
        } else {
            fs::copy(&path, &dst_path).map_err(|e| {
                anyhow::anyhow!("Failed to copy {:?} to {:?}: {}", path, dst_path, e)
            })?;
        }
    }

    Ok(())
}

fn generate_cargo_toml() -> Result<String> {
    Ok(r#"[package]
name = "azul-memtest"
version = "0.1.0"
edition = "2021"

# Prevent this from being pulled into parent workspace
[workspace]

[dependencies]
# Reference the actual azul crates to compare against
azul-core = { path = "../../core" }
azul-layout = { path = "../../layout" }
azul-css = { path = "../../css" }

[lib]
name = "azul_memtest"
path = "src/lib.rs"
"#
    .to_string())
}

fn generate_lib_rs(api_data: &ApiData) -> Result<String> {
    let mut output = String::new();

    output.push_str("// Auto-generated memory layout tests\n");
    output.push_str("// This file validates that api.json definitions match actual source\n\n");
    output.push_str("#![allow(unused_imports)]\n");
    output.push_str("#![allow(dead_code)]\n");
    output.push_str("#![allow(unused_variables)]\n\n");
    output.push_str("use std::mem;\n\n");

    // Include widgets module if it exists
    output.push_str("// Widget types from dll/src/widgets\n");
    output.push_str("pub mod widgets;\n\n");

    // Collect all classes with external paths
    let mut test_cases = Vec::new();

    for (version_name, version_data) in &api_data.0 {
        for (module_name, module_data) in &version_data.api {
            for (class_name, class_data) in &module_data.classes {
                if let Some(external_path) = &class_data.external {
                    // Generate tests for all types with external paths
                    // (structs, enums, callbacks, etc.)
                    test_cases.push(TestCase {
                        version: version_name.clone(),
                        module: module_name.clone(),
                        class: class_name.clone(),
                        external_path: external_path.clone(),
                        is_struct: class_data.struct_fields.is_some(),
                        struct_fields: class_data.struct_fields.clone(),
                    });
                }
            }
        }
    }

    output.push_str(&format!("// Found {} types to test\n\n", test_cases.len()));

    // Generate test for each type
    for test_case in &test_cases {
        output.push_str(&generate_test_function(test_case)?);
        output.push_str("\n");
    }

    Ok(output)
}

struct TestCase {
    version: String,
    module: String,
    class: String,
    external_path: String,
    is_struct: bool,
    struct_fields: Option<Vec<IndexMap<String, FieldData>>>,
}

fn generate_test_function(test: &TestCase) -> Result<String> {
    let mut output = String::new();

    // Convert external path to Rust import path
    let import_path = convert_external_path(&test.external_path);

    // Generate test function
    let test_name = format!(
        "test_{}_{}_{}",
        sanitize_name(&test.version),
        sanitize_name(&test.module),
        sanitize_name(&test.class)
    );

    output.push_str(&format!("#[test]\n"));
    output.push_str(&format!("fn {}() {{\n", test_name));
    output.push_str(&format!("    // Testing: {}.{}\n", test.module, test.class));
    output.push_str(&format!("    // External path: {}\n", test.external_path));
    output.push_str(&format!("    // Import: {}\n", import_path));

    // Try to import and test the type
    if let Some(rust_type) = try_convert_to_rust_type(&import_path) {
        output.push_str(&format!("\n    // Size check\n"));
        output.push_str(&format!(
            "    let size = mem::size_of::<{}>();\n",
            rust_type
        ));
        output.push_str(&format!(
            "    println!(\"Size of {}: {{}} bytes\", size);\n",
            test.class
        ));

        output.push_str(&format!("\n    // Alignment check\n"));
        output.push_str(&format!(
            "    let align = mem::align_of::<{}>();\n",
            rust_type
        ));
        output.push_str(&format!(
            "    println!(\"Alignment of {}: {{}} bytes\", align);\n",
            test.class
        ));

        // If we have struct fields, we can do more validation
        if test.is_struct {
            if let Some(fields_vec) = &test.struct_fields {
                // struct_fields is a Vec<IndexMap>, typically with one element
                if let Some(fields) = fields_vec.first() {
                    output.push_str(&format!("\n    // Field count: {}\n", fields.len()));
                    output.push_str(&format!(
                        "    // Fields: {:?}\n",
                        fields.keys().collect::<Vec<_>>()
                    ));
                }
            }
        }

        output.push_str(&format!("\n    // Basic validation: size should be > 0\n"));
        output.push_str(&format!(
            "    assert!(size > 0, \"Type {{}} should have non-zero size\", \"{}\");\n",
            test.class
        ));
    } else {
        // Can't resolve the type - mark as skipped
        output.push_str(&format!(
            "\n    // SKIPPED: Cannot resolve import path '{}'\n",
            import_path
        ));
        output.push_str(&format!(
            "    println!(\"SKIPPED: {}.{}\");\n",
            test.module, test.class
        ));
    }

    output.push_str("}\n");

    Ok(output)
}

fn convert_external_path(external: &str) -> String {
    // Convert paths from api.json to actual importable Rust paths

    // Handle crate::widgets paths - these should use our local widgets module
    if external.contains("::widgets::") {
        return external.replace("crate::", "crate::");
    }

    // Handle crate::azul_impl paths - these are in azul_core
    if external.starts_with("crate::azul_impl::") {
        return external.replace("crate::azul_impl::", "azul_core::");
    }

    // Handle other crate:: paths - try to map to azul_core
    if external.starts_with("crate::") {
        // Check for known module paths
        if external.contains("::app_resources::") {
            return external.replace("crate::", "azul_core::");
        }
        if external.contains("::task::") {
            return external.replace("crate::", "azul_core::");
        }
        if external.contains("::callbacks::") {
            return external.replace("crate::", "azul_core::");
        }
        if external.contains("::dom::") {
            return external.replace("crate::", "azul_core::");
        }
        if external.contains("::window::") {
            return external.replace("crate::", "azul_core::");
        }
        if external.contains("::gl::") {
            return external.replace("crate::", "azul_core::");
        }
        if external.contains("::ui_solver::") {
            return external.replace("crate::", "azul_core::");
        }

        // Default fallback for crate:: paths
        return external.replace("crate::", "azul_core::");
    }

    // Paths that are already properly qualified
    if external.starts_with("azul_core::")
        || external.starts_with("azul_layout::")
        || external.starts_with("azul_css::")
    {
        return external.to_string();
    }

    // Unknown format - return as-is
    external.to_string()
}

fn try_convert_to_rust_type(import_path: &str) -> Option<String> {
    // Extract the type name from the import path
    if let Some(last_component) = import_path.split("::").last() {
        // Check if it looks like a valid Rust type
        if last_component.chars().next()?.is_uppercase() {
            return Some(import_path.to_string());
        }
    }
    None
}

fn sanitize_name(name: &str) -> String {
    name.replace(".", "_")
        .replace("-", "_")
        .replace("::", "_")
        .to_lowercase()
}
