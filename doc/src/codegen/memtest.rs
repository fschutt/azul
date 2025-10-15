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
    let generated_type = format!("crate::generated::Az{}", test.class);
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
    let generated_type = format!("crate::generated::Az{}", test.class); // TODO: Use version-based prefix
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

    // Compare each discriminant - only check generated types
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
    output.push_str("use core::ffi::c_void;\n\n");

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

    // Wrap everything in a dll module (private, contains all versioned types)
    output.push_str("mod dll {\n");
    output.push_str("    // In std environment, alloc types are re-exported from std\n");
    output.push_str("    use core::ffi::c_void;\n");
    output.push_str("    use std::vec::Vec;\n");
    output.push_str("    use std::string::String;\n\n");

    // Indent all generated code
    for line in generated_code.lines() {
        if !line.is_empty() {
            output.push_str("    ");
        }
        output.push_str(line);
        output.push_str("\n");
    }

    output.push_str("}\n\n");

    // Add public API modules with re-exports and patches
    output.push_str(&generate_public_api_modules(version_data, &prefix)?);

    Ok(output)
}

/// Generate public API modules with re-exports and patches
fn generate_public_api_modules(version_data: &VersionData, prefix: &str) -> Result<String> {
    let patch_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src/codegen/api-patch");

    // Map patch files to their module names
    let patches = vec![
        ("string.rs", "str"),
        ("vec.rs", "vec"),
        ("option.rs", "option"),
        ("dom.rs", "dom"),
        ("gl.rs", "gl"),
        ("css.rs", "css"),
        ("window.rs", "window"),
        ("callbacks.rs", "callbacks"),
    ];

    let mut output = String::new();
    output.push_str("// ===== Public API Modules =====\n");
    output.push_str("// Each module re-exports types from dll with friendly names\n\n");

    for (patch_file, module_name) in patches {
        output.push_str(&format!("pub mod {} {{\n", module_name));
        output.push_str("    use core::ffi::c_void;\n");
        output.push_str("    use super::dll::*;\n\n");

        // Generate re-exports: pub use Az1Type as Type;
        output.push_str(&generate_reexports(version_data, prefix, module_name)?);

        // Add patches
        let patch_path = format!("{}/{}", patch_dir, patch_file);
        if let Ok(patch_content) = fs::read_to_string(&patch_path) {
            output.push_str("\n    // ===== Trait Implementations =====\n\n");
            output.push_str(&process_patch_content(&patch_content, prefix)?);
        }

        output.push_str("}\n\n");
    }

    Ok(output)
}

/// Generate re-exports for a module: pub use Az1Type as Type;
fn generate_reexports(
    version_data: &VersionData,
    prefix: &str,
    module_name: &str,
) -> Result<String> {
    let mut output = String::new();
    output.push_str("    // Re-export types with friendly names\n");

    if let Some(module_data) = version_data.api.get(module_name) {
        for class_name in module_data.classes.keys() {
            output.push_str(&format!(
                "    pub use super::dll::{}{} as {};\n",
                prefix, class_name, class_name
            ));
        }
    }

    Ok(output)
}

/// Process patch content: skip use statements, replace type names
fn process_patch_content(patch_content: &str, prefix: &str) -> Result<String> {
    let mut output = String::new();
    let mut skip_until_end_brace = false;

    for line in patch_content.lines() {
        let trimmed = line.trim();

        // Skip use statements that would conflict
        if trimmed.starts_with("use alloc::vec")
            || trimmed.starts_with("use std::vec")
            || trimmed.starts_with("use alloc::string")
            || trimmed.starts_with("use std::string")
            || trimmed.starts_with("use crate::dll")
            || trimmed.starts_with("use crate::gl")
            || trimmed.starts_with("use crate::vec")
            || trimmed.starts_with("use crate::option")
            || trimmed.starts_with("use crate::prelude")
        {
            if trimmed.contains("{") && !trimmed.contains("};") {
                skip_until_end_brace = true;
            }
            continue;
        }

        if skip_until_end_brace {
            if trimmed.contains("};") {
                skip_until_end_brace = false;
            }
            continue;
        }

        // Adjust content
        let mut adjusted_line = line.replace("alloc::", "std::");

        // Replace Az-prefixed types FIRST (before module path replacements)
        // This ensures that crate::str::String becomes crate::str::Az1String
        // before we change crate:: to super::
        adjusted_line = adjusted_line.replace(" Az", &format!(" {}", prefix));
        adjusted_line = adjusted_line.replace("(Az", &format!("({}", prefix));
        adjusted_line = adjusted_line.replace("<Az", &format!("<{}", prefix));
        adjusted_line = adjusted_line.replace(":Az", &format!(":{}", prefix));
        adjusted_line = adjusted_line.replace(",Az", &format!(",{}", prefix));

        // NOW replace crate:: paths that don't work in generated.rs
        // In generated.rs, we have: mod dll { } and pub mod vec { }, pub mod str { }, etc.
        // So crate::dll:: needs to be super::dll:: (when inside pub mod vec)
        // At this point, types are already prefixed (e.g., crate::str::Az1String)
        adjusted_line = adjusted_line.replace("crate::dll::", "super::dll::");
        adjusted_line = adjusted_line.replace("crate::vec::", "super::vec::");
        adjusted_line = adjusted_line.replace("crate::str::", "super::str::");
        adjusted_line = adjusted_line.replace("crate::prelude::", "");

        // Replace non-Az types that need prefix
        adjusted_line = adjusted_line.replace(" CssProperty", &format!(" {}CssProperty", prefix));
        adjusted_line = adjusted_line.replace("(CssProperty", &format!("({}CssProperty", prefix));
        adjusted_line = adjusted_line.replace("<CssProperty", &format!("<{}CssProperty", prefix));
        adjusted_line =
            adjusted_line.replace(" CssPropertyType", &format!(" {}CssPropertyType", prefix));
        adjusted_line = adjusted_line.replace(" PixelValue", &format!(" {}PixelValue", prefix));
        adjusted_line = adjusted_line.replace("(PixelValue", &format!("({}PixelValue", prefix));
        adjusted_line = adjusted_line.replace(" Dom", &format!(" {}Dom", prefix));
        adjusted_line = adjusted_line.replace("(Dom", &format!("({}Dom", prefix));
        adjusted_line = adjusted_line.replace("<Dom", &format!("<{}Dom", prefix));

        // Add more type replacements as needed
        for type_name in &[
            // Original types
            "StyleBoxShadowValue",
            "FloatValue",
            "SizeMetric",
            "AngleMetric",
            "LayoutOverflowValue",
            "StyleFilterVecValue",
            "NodeData",
            "U8VecRef",
            "StyleWordSpacingValue",
            "StyleTransformVecValue",
            "StyleTransformOriginValue",
            "StyleTextColorValue",
            "StyleTextAlignValue",
            "StyleTabWidthValue",
            "StylePerspectiveOriginValue",
            "StyleOpacityValue",
            // Style*Value types
            "StyleMixBlendModeValue",
            "StyleLineHeightValue",
            "StyleLetterSpacingValue",
            "StyleFontSizeValue",
            "StyleFontFamilyVecValue",
            "StyleCursorValue",
            "StyleBorderTopStyleValue",
            "StyleBorderTopRightRadiusValue",
            "StyleBorderTopLeftRadiusValue",
            "StyleBorderTopColorValue",
            "StyleBorderRightStyleValue",
            "StyleBorderRightColorValue",
            "StyleBorderLeftStyleValue",
            "StyleBorderLeftColorValue",
            "StyleBorderBottomStyleValue",
            "StyleBorderBottomRightRadiusValue",
            "StyleBorderBottomLeftRadiusValue",
            "StyleBorderBottomColorValue",
            "StyleBackfaceVisibilityValue",
            "StyleBackgroundContentVecValue",
            "StyleBackgroundPositionVecValue",
            "StyleBackgroundRepeatVecValue",
            "StyleBackgroundSizeVecValue",
            // Layout*Value types
            "LayoutAlignContentValue",
            "LayoutAlignItemsValue",
            "LayoutBorderBottomWidthValue",
            "LayoutBorderLeftWidthValue",
            "LayoutBorderRightWidthValue",
            "LayoutBorderTopWidthValue",
            "LayoutBottomValue",
            "LayoutBoxSizingValue",
            "LayoutDisplayValue",
            "LayoutFlexDirectionValue",
            "LayoutFlexGrowValue",
            "LayoutFlexShrinkValue",
            "LayoutFlexWrapValue",
            "LayoutFloatValue",
            "LayoutHeightValue",
            "LayoutJustifyContentValue",
            "LayoutLeftValue",
            "LayoutMarginBottomValue",
            "LayoutMarginLeftValue",
            "LayoutMarginRightValue",
            "LayoutMarginTopValue",
            "LayoutMaxHeightValue",
            "LayoutMaxWidthValue",
            "LayoutMinHeightValue",
            "LayoutMinWidthValue",
            "LayoutPaddingBottomValue",
            "LayoutPaddingLeftValue",
            "LayoutPaddingRightValue",
            "LayoutPaddingTopValue",
            "LayoutPositionValue",
            "LayoutRightValue",
            "LayoutTopValue",
            "LayoutWidthValue",
            // Other types
            "LayoutPoint",
            "LayoutSize",
            "NodeType",
            "OptionRefAny",
            "OptionTabIndex",
            "PercentageValue",
            "ScrollbarStyleValue",
            "TabIndex",
            "AngleValue",
            "CallbackDataVec",
            "IdOrClassVec",
            "IdOrClassVecDestructor",
        ] {
            adjusted_line = adjusted_line.replace(
                &format!(" {}", type_name),
                &format!(" {}{}", prefix, type_name),
            );
            adjusted_line = adjusted_line.replace(
                &format!("({}", type_name),
                &format!("({}{}", prefix, type_name),
            );
        }

        if !adjusted_line.trim().is_empty() {
            output.push_str("    ");
        }
        output.push_str(&adjusted_line);
        output.push_str("\n");
    }

    Ok(output)
}

fn load_api_patches_inline(prefix: &str) -> Result<String> {
    let patch_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src/codegen/api-patch");

    let patches = vec![
        "string.rs",
        "vec.rs",
        "option.rs",
        "dom.rs",
        "gl.rs",
        "css.rs",
        "window.rs",
        "callbacks.rs",
    ];

    let mut output = String::new();

    for patch_file in patches {
        let patch_path = format!("{}/{}", patch_dir, patch_file);
        if let Ok(patch_content) = fs::read_to_string(&patch_path) {
            output.push_str(&format!("    // ===== From {} =====\n", patch_file));

            // Adjust content for inline use in dll module:
            // - alloc:: → std::
            // - Az → Az1 (or whatever prefix)
            // - Remove crate::dll:: imports (we're already in dll)
            // - Remove crate::gl:: imports (GL types are here)
            // - Skip wildcard imports
            let mut skip_until_end_brace = false;

            for line in patch_content.lines() {
                let trimmed = line.trim();

                // Skip use statements that would conflict with dll module's imports
                if trimmed.starts_with("use alloc::vec::")
                    || trimmed.starts_with("use std::vec::")
                    || trimmed.starts_with("use alloc::string::")
                    || trimmed.starts_with("use std::string::")
                    || trimmed.starts_with("use crate::dll")
                    || trimmed.starts_with("use crate::gl")
                    || trimmed.starts_with("use crate::vec")
                    || trimmed.starts_with("use crate::option")
                    || trimmed.starts_with("use crate::prelude")
                {
                    // Check if it's a multi-line use statement
                    if trimmed.contains("{") && !trimmed.contains("};") {
                        skip_until_end_brace = true;
                    }
                    continue; // Skip this line
                }

                if skip_until_end_brace {
                    if trimmed.contains("};") {
                        skip_until_end_brace = false;
                    }
                    continue;
                }

                // Keep other use statements (core::fmt, core::cmp, alloc::slice, etc.)
                // but adjust their content
                let mut adjusted_line = line.replace("alloc::", "std::");

                // First do the generic Az prefix replacement
                adjusted_line = adjusted_line.replace(" Az", &format!(" {}", prefix));
                adjusted_line = adjusted_line.replace("(Az", &format!("({}", prefix));
                adjusted_line = adjusted_line.replace("<Az", &format!("<{}", prefix));
                adjusted_line = adjusted_line.replace(":Az", &format!(":{}", prefix));
                adjusted_line = adjusted_line.replace(",Az", &format!(",{}", prefix));

                // Then replace type names that don't start with Az
                // Using word boundaries to avoid replacing parts of words
                adjusted_line =
                    adjusted_line.replace(" CssProperty", &format!(" {}CssProperty", prefix));
                adjusted_line =
                    adjusted_line.replace("(CssProperty", &format!("({}CssProperty", prefix));
                adjusted_line =
                    adjusted_line.replace("<CssProperty", &format!("<{}CssProperty", prefix));
                adjusted_line = adjusted_line
                    .replace(" CssPropertyType", &format!(" {}CssPropertyType", prefix));
                adjusted_line =
                    adjusted_line.replace(" PixelValue", &format!(" {}PixelValue", prefix));
                adjusted_line =
                    adjusted_line.replace("(PixelValue", &format!("({}PixelValue", prefix));
                adjusted_line = adjusted_line.replace(
                    " StyleBoxShadowValue",
                    &format!(" {}StyleBoxShadowValue", prefix),
                );
                adjusted_line =
                    adjusted_line.replace(" FloatValue", &format!(" {}FloatValue", prefix));
                adjusted_line =
                    adjusted_line.replace(" SizeMetric", &format!(" {}SizeMetric", prefix));
                adjusted_line =
                    adjusted_line.replace(" AngleMetric", &format!(" {}AngleMetric", prefix));
                adjusted_line = adjusted_line.replace(
                    " LayoutOverflowValue",
                    &format!(" {}LayoutOverflowValue", prefix),
                );
                adjusted_line = adjusted_line.replace(
                    " StyleFilterVecValue",
                    &format!(" {}StyleFilterVecValue", prefix),
                );
                adjusted_line = adjusted_line.replace(" NodeData", &format!(" {}NodeData", prefix));
                adjusted_line = adjusted_line.replace(" Dom", &format!(" {}Dom", prefix));
                adjusted_line = adjusted_line.replace("(Dom", &format!("({}Dom", prefix));
                adjusted_line = adjusted_line.replace("<Dom", &format!("<{}Dom", prefix));
                adjusted_line = adjusted_line.replace(" U8VecRef", &format!(" {}U8VecRef", prefix));

                // In generated.rs we're inside dll module, so crate::dll:: is just self::
                // But since all patches are inlined into dll, we can just remove the prefix
                adjusted_line = adjusted_line.replace("crate::dll::", "");
                adjusted_line = adjusted_line.replace("crate::prelude::", "");
                adjusted_line = adjusted_line.replace("crate::vec::", "");
                adjusted_line = adjusted_line.replace("crate::option::", "");

                if !adjusted_line.trim().is_empty() {
                    output.push_str("    ");
                }
                output.push_str(&adjusted_line);
                output.push_str("\n");
            }
            output.push_str("\n");
        }
    }

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
