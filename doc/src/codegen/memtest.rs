// Code generation for memory layout tests
// This module generates a complete test crate that validates memory layouts

use std::{fs, path::Path};

use anyhow::Result;
use indexmap::IndexMap;

use super::struct_gen::{generate_structs, GenerateConfig, StructMetadata};
use crate::api::*;

/// Generate a test crate that validates memory layouts
pub fn generate_memtest_crate(api_data: &ApiData, project_root: &Path) -> Result<()> {
    let memtest_dir = project_root.join("target").join("memtest");

    // Create directory structure
    fs::create_dir_all(&memtest_dir)?;
    fs::create_dir_all(memtest_dir.join("src"))?;

    // Generate Cargo.toml
    let cargo_toml = generate_cargo_toml()?;
    fs::write(memtest_dir.join("Cargo.toml"), cargo_toml)?;

    // Generate generated.rs with all API types
    let generated_rs = generate_generated_rs(api_data)?;
    fs::write(memtest_dir.join("src").join("generated.rs"), generated_rs)?;

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
    output.push_str("pub mod generated;\n\n");

    // Collect all test cases
    let mut test_cases = Vec::new();

    for (version_name, version_data) in &api_data.0 {
        for (module_name, module_data) in &version_data.api {
            for (class_name, class_data) in &module_data.classes {
                if let Some(external_path) = &class_data.external {
                    test_cases.push(TestCase {
                        version: version_name.clone(),
                        module: module_name.clone(),
                        class: class_name.clone(),
                        external_path: external_path.clone(),
                        has_struct: class_data.struct_fields.is_some(),
                        has_enum: class_data.enum_fields.is_some(),
                        enum_fields: class_data.enum_fields.clone(),
                    });
                }
            }
        }
    }

    output.push_str(&format!("// Found {} types to test\n\n", test_cases.len()));

    // Generate test for each type
    for test_case in &test_cases {
        output.push_str(&generate_size_and_align_test(&test_case)?);
        output.push_str("\n");

        // Generate discriminant test for enums
        if test_case.has_enum {
            if let Some(enum_fields) = &test_case.enum_fields {
                output.push_str(&generate_discriminant_test(&test_case, enum_fields)?);
                output.push_str("\n");
            }
        }
    }

    Ok(output)
}

struct TestCase {
    version: String,
    module: String,
    class: String,
    external_path: String,
    has_struct: bool,
    has_enum: bool,
    enum_fields: Option<Vec<IndexMap<String, EnumVariantData>>>,
}

fn generate_size_and_align_test(test: &TestCase) -> Result<String> {
    let mut output = String::new();

    let test_name = format!(
        "test_size_align_{}_{}_{}",
        sanitize_name(&test.version),
        sanitize_name(&test.module),
        sanitize_name(&test.class)
    );

    // Add "Az" prefix for generated types
    let generated_type = format!("generated::Az{}", test.class);
    let external_type = &test.external_path;

    output.push_str(&format!("/// Test size and alignment of {}\n", test.class));
    output.push_str(&format!("#[test]\n"));
    output.push_str(&format!("fn {}() {{\n", test_name));
    output.push_str(&format!(
        "    let generated_size = mem::size_of::<{}>();\n",
        generated_type
    ));
    output.push_str(&format!(
        "    let external_size = mem::size_of::<{}>();\n",
        external_type
    ));
    output.push_str(&format!(
        "    let generated_align = mem::align_of::<{}>();\n",
        generated_type
    ));
    output.push_str(&format!(
        "    let external_align = mem::align_of::<{}>();\n",
        external_type
    ));
    output.push_str(&format!("\n"));
    output.push_str(&format!("    assert_eq!(generated_size, external_size, \n"));
    output.push_str(&format!(
        "        \"Size mismatch for {}: generated={{}} bytes, external={{}} bytes\",\n",
        test.class
    ));
    output.push_str(&format!("        generated_size, external_size\n"));
    output.push_str(&format!("    );\n"));
    output.push_str(&format!("\n"));
    output.push_str(&format!(
        "    assert_eq!(generated_align, external_align,\n"
    ));
    output.push_str(&format!(
        "        \"Alignment mismatch for {}: generated={{}} bytes, external={{}} bytes\",\n",
        test.class
    ));
    output.push_str(&format!("        generated_align, external_align\n"));
    output.push_str(&format!("    );\n"));
    output.push_str(&format!("}}\n"));

    Ok(output)
}

fn generate_discriminant_test(
    test: &TestCase,
    enum_fields: &Vec<IndexMap<String, EnumVariantData>>,
) -> Result<String> {
    let mut output = String::new();

    let test_name = format!(
        "test_discriminant_{}_{}_{}",
        sanitize_name(&test.version),
        sanitize_name(&test.module),
        sanitize_name(&test.class)
    );

    // Add prefix for generated types (e.g., "Az1" for first version)
    let generated_type = format!("generated::Az{}", test.class); // TODO: Use version-based prefix
    let external_type = &test.external_path;

    output.push_str(&format!("/// Test discriminant order of {}\n", test.class));
    output.push_str(&format!("#[test]\n"));
    output.push_str(&format!("fn {}() {{\n", test_name));
    output.push_str(&format!("    unsafe {{\n"));

    // Generate instances for both types
    let variant_count = enum_fields.len();
    for (idx, variant_map) in enum_fields.iter().enumerate() {
        for (variant_name, _) in variant_map {
            output.push_str(&format!(
                "        let generated_{}: {} = mem::MaybeUninit::uninit().assume_init();\n",
                idx, generated_type
            ));
            output.push_str(&format!(
                "        let external_{}: {} = mem::MaybeUninit::uninit().assume_init();\n",
                idx, external_type
            ));
        }
    }

    output.push_str(&format!("\n"));

    // Get discriminants
    for idx in 0..variant_count {
        output.push_str(&format!(
            "        let gen_disc_{} = mem::discriminant(&generated_{});\n",
            idx, idx
        ));
        output.push_str(&format!(
            "        let ext_disc_{} = mem::discriminant(&external_{});\n",
            idx, idx
        ));
    }

    output.push_str(&format!("\n"));
    output.push_str(&format!("        // Compare discriminants pairwise\n"));

    // Compare each discriminant
    for i in 0..variant_count {
        for j in (i + 1)..variant_count {
            let mut comment = String::new();
            if let Some(variant_map_i) = enum_fields.get(i) {
                if let Some(variant_map_j) = enum_fields.get(j) {
                    if let Some((name_i, _)) = variant_map_i.iter().next() {
                        if let Some((name_j, _)) = variant_map_j.iter().next() {
                            comment = format!(" // {} != {}", name_i, name_j);
                        }
                    }
                }
            }
            output.push_str(&format!(
                "        assert_ne!(gen_disc_{}, gen_disc_{});{}\n",
                i, j, comment
            ));
            output.push_str(&format!(
                "        assert_ne!(ext_disc_{}, ext_disc_{});\n",
                i, j
            ));
        }
    }

    output.push_str(&format!("    }}\n"));
    output.push_str(&format!("}}\n"));

    Ok(output)
}

fn sanitize_name(name: &str) -> String {
    name.replace(".", "_")
        .replace("-", "_")
        .replace("::", "_")
        .to_lowercase()
}

fn generate_generated_rs(api_data: &ApiData) -> Result<String> {
    let mut output = String::new();

    output.push_str("// Auto-generated API definitions from api.json\n");
    output
        .push_str("// These types should match the memory layout of the actual implementation\n\n");
    output.push_str("#![allow(dead_code)]\n");
    output.push_str("#![allow(non_camel_case_types)]\n");
    output.push_str("#![allow(non_snake_case)]\n\n");

    // Use the latest version (or first version found)
    let version_data = api_data
        .0
        .values()
        .next()
        .ok_or_else(|| anyhow::anyhow!("No API version found"))?;

    // Determine version prefix based on date
    let version_name = api_data
        .0
        .keys()
        .next()
        .ok_or_else(|| anyhow::anyhow!("No version name found"))?;
    let prefix = api_data
        .get_version_prefix(version_name)
        .unwrap_or_else(|| "Az".to_string());

    // Collect all classes to generate with StructMetadata
    let mut structs_map = std::collections::HashMap::new();

    for (module_name, module_data) in &version_data.api {
        for (class_name, class_data) in &module_data.classes {
            // Extract metadata for this class
            let metadata = StructMetadata::from_class_data(class_name.clone(), class_data);

            // Store with prefix (e.g., "Az1Point", "Az2Point", etc.)
            let prefixed_name = format!("{}{}", prefix, class_name);
            structs_map.insert(prefixed_name, metadata);
        }
    }

    // Configure generation for generated.rs
    let config = GenerateConfig {
        prefix: prefix.clone(),
        indent: 0,
        autoderive: true,
        private_pointers: false,
        no_derive: false,
        wrapper_postfix: "".to_string(),
    };

    // Generate all structs/enums using struct_gen module
    let generated_code = generate_structs(version_data, &structs_map, &config)?;

    output.push_str(&generated_code);

    Ok(output)
}

fn generate_class_definition(
    class_name: &str,
    class_data: &ClassData,
    class_map: &std::collections::HashMap<String, ClassData>,
) -> Result<String> {
    // This function is no longer used - kept for compatibility
    // All generation is now handled by struct_gen module
    Ok(String::new())
}
