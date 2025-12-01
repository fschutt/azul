// Code generation for memory layout tests
// This module generates a complete test crate that validates memory layouts

use std::{collections::HashMap, fs, path::Path};

use indexmap::IndexMap;
use regex::Regex;

use super::{
    func_gen::build_functions_map,
    struct_gen::{generate_structs, GenerateConfig, StructMetadata},
};
use crate::api::*;

pub type Result<T> = std::result::Result<T, String>;

/// Pre-compiled regexes used throughout memtest generation
#[derive(Debug)]
pub struct CompiledRegexes {
    /// Matches "Az" followed by a capital letter (type names and functions)
    pub type_pattern: Regex,
    /// Map of type name -> pre-compiled regex for word boundary matching
    pub type_regexes: HashMap<String, Regex>,
}

impl CompiledRegexes {
    pub fn new(version_data: &VersionData) -> Result<Self> {
        println!("      [BUILD] Compiling type replacement regexes...");

        // Collect all type names from api.json
        let mut all_types = std::collections::HashSet::new();
        for module_data in version_data.api.values() {
            for class_name in module_data.classes.keys() {
                all_types.insert(class_name.clone());
            }
        }

        // Pre-compile a regex for each type (for whole-word matching)
        let mut type_regexes = HashMap::new();
        for type_name in &all_types {
            // Skip special cases and primitive types
            if type_name == "String" || type_name == "Vec" {
                continue;
            }
            // Skip primitive types that should never get a prefix
            if PRIMITIVE_TYPES.contains(&type_name.as_str()) {
                continue;
            }
            // Skip single-letter generic type parameters (e.g., T, U, V)
            if is_generic_type_param(type_name) {
                continue;
            }

            let pattern = format!(r"\b{}\b", regex::escape(type_name));
            if let Ok(re) = Regex::new(&pattern) {
                type_regexes.insert(type_name.clone(), re);
            }
        }

        println!("      [OK] Compiled {} type regexes", type_regexes.len());

        Ok(Self {
            type_pattern: Regex::new(r"\bAz([A-Z][a-zA-Z0-9_]*)")
                .map_err(|e| format!("Failed to compile type_pattern regex: {}", e))?,
            type_regexes,
        })
    }
}

/// Configuration for memtest generation
#[derive(Debug, Clone)]
pub struct MemtestConfig {
    /// Remove serde-support feature gates
    pub remove_serde: bool,
    /// Remove other optional features
    pub remove_optional_features: Vec<String>,
}

impl Default for MemtestConfig {
    fn default() -> Self {
        Self {
            remove_serde: false, // Keep serde lines, we add serde deps to Cargo.toml
            remove_optional_features: vec![],
        }
    }
}

/// Generate a test crate that validates memory layouts
pub fn generate_memtest_crate(api_data: &ApiData, project_root: &Path) -> Result<()> {
    println!("  [DIR] Setting up directories...");

    let config = MemtestConfig::default();
    let memtest_dir = project_root.join("target").join("memtest");

    // Get version data for regex compilation
    let version_name = api_data
        .0
        .keys()
        .next()
        .ok_or_else(|| "No version name found".to_string())?;
    let version_data = api_data
        .get_version(version_name)
        .ok_or_else(|| "No API version found".to_string())?;

    // Compile all regexes once at the start
    println!("  [BUILD] Compiling regex patterns...");
    let regexes = CompiledRegexes::new(version_data)?;

    // Create directory structure
    fs::create_dir_all(&memtest_dir).map_err(|e| format!("Failed to create memtest dir: {}", e))?;
    fs::create_dir_all(memtest_dir.join("src"))
        .map_err(|e| format!("Failed to create src dir: {}", e))?;

    println!("  [NOTE] Generating Cargo.toml...");
    // Generate Cargo.toml
    let cargo_toml = generate_cargo_toml()?;
    fs::write(memtest_dir.join("Cargo.toml"), cargo_toml)
        .map_err(|e| format!("Failed to write Cargo.toml: {}", e))?;

    println!("  [FIX] Generating generated.rs (this may take a while)...");
    // Generate generated.rs with all API types
    let generated_rs = generate_generated_rs(api_data, &config, &regexes)?;
    println!(
        "  [SAVE] Writing generated.rs ({} bytes)...",
        generated_rs.len()
    );
    fs::write(memtest_dir.join("src").join("generated.rs"), generated_rs)
        .map_err(|e| format!("Failed to write generated.rs: {}", e))?;

    println!("  [TEST] Generating lib.rs with tests...");
    // Generate lib.rs with all tests
    let lib_rs = generate_lib_rs(api_data)?;
    fs::write(memtest_dir.join("src").join("lib.rs"), lib_rs)
        .map_err(|e| format!("Failed to write lib.rs: {}", e))?;

    println!(
        "[OK] Generated memory test crate at: {}",
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

# Serde support (not enabled by default, but types need to be available for conditional compilation)
serde = { version = "1.0", optional = true }
serde_derive = { version = "1.0", optional = true }

[lib]
name = "azul_memtest"
path = "src/lib.rs"

[features]
serde-support = ["serde", "serde_derive"]
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

fn generate_generated_rs(
    api_data: &ApiData,
    config: &MemtestConfig,
    regexes: &CompiledRegexes,
) -> Result<String> {
    println!("    [WAIT] Starting generated.rs creation...");
    let mut output = String::new();

    output.push_str("// Auto-generated API definitions from api.json for memtest\n");
    output.push_str(
        "#![allow(dead_code, unused_imports, non_camel_case_types, non_snake_case, unused_unsafe, \
         clippy::all)]\n\n",
    );
    output.push_str("use core::ffi::c_void;\n\n");

    let version_name = api_data
        .0
        .keys()
        .next()
        .ok_or_else(|| "No version name found".to_string())?;
    let version_data = api_data
        .get_version(version_name)
        .ok_or_else(|| "No API version found".to_string())?;
    let prefix = api_data
        .get_version_prefix(version_name)
        .unwrap_or_else(|| "Az".to_string());

    println!("    [BUILD] Generating dll module...");
    // 1. Generate the `dll` module containing raw structs AND function stubs
    output.push_str(&generate_dll_module(
        version_data,
        &prefix,
        config,
        regexes,
    )?);

    println!("    [PKG] Generating public API modules...");
    // 2. Generate the public API modules (`pub mod str`, etc.)
    output.push_str(&generate_public_api_modules(
        version_data,
        &prefix,
        config,
        regexes,
    )?);

    println!("    [OK] Generated.rs creation complete");
    Ok(output)
}

fn generate_dll_module(
    version_data: &VersionData,
    prefix: &str,
    config: &MemtestConfig,
    regexes: &CompiledRegexes,
) -> Result<String> {
    println!("      [BUILD]  Building dll module...");
    let mut dll_code = String::new();
    dll_code.push_str("pub mod dll {\n");
    dll_code.push_str("    use super::c_void;\n");
    dll_code.push_str("    use std::{string, vec, slice, mem, fmt, cmp, hash, iter};\n\n");

    println!("      [STATS] Collecting structs...");
    // Collect all structs for this version
    // Use entry API to prefer versions with struct_fields/enum_fields over empty external references
    let mut structs_map: HashMap<String, StructMetadata> = HashMap::new();
    for (_module_name, module_data) in &version_data.api {
        for (class_name, class_data) in &module_data.classes {
            // Skip primitive types and generic type parameters
            if PRIMITIVE_TYPES.contains(&class_name.as_str()) || is_generic_type_param(class_name) {
                continue;
            }
            // Always add prefix, even if type already has it (consistency)
            let prefixed_name = format!("{}{}", prefix, class_name);
            
            // Check if this class has actual content (struct_fields or enum_fields)
            let has_content = class_data.struct_fields.is_some() 
                || class_data.enum_fields.is_some() 
                || class_data.callback_typedef.is_some()
                || class_data.type_alias.is_some();
            
            // Only insert if:
            // 1. The type doesn't exist yet, or
            // 2. The new version has content and the existing one doesn't
            if let Some(existing) = structs_map.get(&prefixed_name) {
                let existing_has_content = existing.struct_fields.is_some() 
                    || existing.enum_fields.is_some()
                    || existing.callback_typedef.is_some()
                    || existing.type_alias.is_some();
                if has_content && !existing_has_content {
                    let metadata = StructMetadata::from_class_data(class_name.clone(), class_data);
                    structs_map.insert(prefixed_name, metadata);
                }
                // else: keep existing (it either has content or both don't)
            } else {
                let metadata = StructMetadata::from_class_data(class_name.clone(), class_data);
                structs_map.insert(prefixed_name, metadata);
            }
        }
    }
    println!("      📚 Found {} types", structs_map.len());

    println!("      [FIX] Generating struct definitions...");
    // Generate all struct/enum/type definitions inside the dll module
    let struct_config = GenerateConfig {
        prefix: prefix.to_string(),
        indent: 4,
        autoderive: true,
        private_pointers: false,
        no_derive: false,
        wrapper_postfix: String::new(),
    };
    dll_code.push_str(
        &generate_structs(version_data, &structs_map, &struct_config).map_err(|e| e.to_string())?,
    );

    println!("      [TARGET] Generating function stubs...");
    // Generate unimplemented!() stubs for all exported C functions
    let functions_map = build_functions_map(version_data, prefix).map_err(|e| e.to_string())?;
    println!("      [LINK] Found {} functions", functions_map.len());
    dll_code.push_str("\n    // --- C-ABI Function Stubs ---\n");
    for (fn_name, (fn_args, fn_return)) in &functions_map {
        let return_str = if fn_return.is_empty() {
            "".to_string()
        } else {
            format!(" -> {}", fn_return)
        };

        dll_code.push_str(&format!(
            "    #[allow(unused_variables)]\n    pub unsafe extern \"C\" fn {}({}){} {{ \
             unimplemented!(\"{}\") }}\n",
            fn_name, fn_args, return_str, fn_name
        ));
    }
    dll_code.push_str("    // --- End C-ABI Function Stubs ---\n\n");

    // Add patches from api-patch/dll.rs
    let patch_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src/codegen/api-patch");
    let dll_patch_path = format!("{}/dll.rs", patch_dir);
    if let Ok(patch_content) = fs::read_to_string(&dll_patch_path) {
        dll_code.push_str("    // ===== Trait Implementations (from dll.rs patch) =====\n\n");
        let processed =
            process_patch_content(&patch_content, prefix, version_data, config, regexes)?;
        // Add 4-space indentation to match dll module
        for line in processed.lines() {
            if line.trim().is_empty() {
                dll_code.push('\n');
            } else {
                dll_code.push_str("    ");
                dll_code.push_str(line);
                dll_code.push('\n');
            }
        }
        dll_code.push('\n');
    }

    dll_code.push_str("}\n\n");
    Ok(dll_code)
}

/// Generate public API modules with re-exports and patches
fn generate_public_api_modules(
    version_data: &VersionData,
    prefix: &str,
    config: &MemtestConfig,
    regexes: &CompiledRegexes,
) -> Result<String> {
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
        // ("callbacks.rs", "callbacks"), // Excluded: callback types need workspace search
        // fallback
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
            output.push_str(&process_patch_content(
                &patch_content,
                prefix,
                version_data,
                config,
                regexes,
            )?);
        }

        output.push_str("}\n\n");
    }

    Ok(output)
}

/// Primitive types that should never get an Az prefix
const PRIMITIVE_TYPES: &[&str] = &[
    "bool", "f32", "f64", "fn", "i128", "i16", "i32", "i64", "i8", "isize", 
    "slice", "u128", "u16", "u32", "u64", "u8", "usize", "c_void",
    "str", "char", "c_char", "c_schar", "c_uchar",
];

/// Single-letter types are usually generic type parameters
fn is_generic_type_param(type_name: &str) -> bool {
    type_name.len() == 1 && type_name.chars().next().map(|c| c.is_ascii_uppercase()).unwrap_or(false)
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
            // Skip primitive types and generic type parameters - they don't need re-exports
            if PRIMITIVE_TYPES.contains(&class_name.as_str()) || is_generic_type_param(class_name) {
                continue;
            }
            output.push_str(&format!(
                "    pub use super::dll::{}{} as {};\n",
                prefix, class_name, class_name
            ));
        }
    }

    Ok(output)
}

/// Process patch content: skip use statements, replace type names, remove serde
fn process_patch_content(
    patch_content: &str,
    prefix: &str,
    version_data: &VersionData,
    config: &MemtestConfig,
    regexes: &CompiledRegexes,
) -> Result<String> {
    println!(
        "      [SEARCH] Processing patch content ({} bytes)...",
        patch_content.len()
    );

    let mut output = String::new();
    let mut skip_until_end_brace = false;
    let mut line_count = 0;

    for line in patch_content.lines() {
        line_count += 1;
        if line_count % 100 == 0 {
            println!("        [WAIT] Processed {} lines...", line_count);
        }

        let trimmed = line.trim();

        // Skip lines with serde-support feature if configured
        if config.remove_serde {
            if trimmed.contains("serde-support") || trimmed.contains("serde_support") {
                continue;
            }
        }

        // Skip other optional features
        for feature in &config.remove_optional_features {
            if trimmed.contains(feature) {
                continue;
            }
        }

        // Skip use statements that would conflict in the memtest context
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
            // Handle multi-line use statements
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

        // Start with the line as-is
        let mut adjusted_line = line.to_string();

        // Replace alloc:: with std:: (memtest doesn't use no_std)
        adjusted_line = adjusted_line.replace("alloc::", "std::");

        // Fix bare module references that need std:: prefix
        // Replace "string::String" with "std::string::String" (but not "super::str::String")
        adjusted_line = adjusted_line.replace("string::String", "std::string::String");
        // Replace "vec::Vec" with "std::vec::Vec" (but not "super::vec::")
        adjusted_line = adjusted_line.replace("vec::Vec", "std::vec::Vec");

        // FIRST: Fix unprefixed type names that exist in api.json
        // For example: StyleFilterVec -> Az1StyleFilterVec, DomVec -> Az1DomVec
        // Do this BEFORE the Az-> Az1 conversion to avoid double-prefixing
        // Use pre-compiled regexes for massive speedup
        for (type_name, re) in &regexes.type_regexes {
            let prefixed = format!("{}{}", prefix, type_name);
            adjusted_line = re
                .replace_all(&adjusted_line, prefixed.as_str())
                .to_string();
        }

        // SECOND: Use regex to replace all remaining Az-prefixed types with the versioned prefix
        // This prevents double-prefixing: Az -> Az1 (not Az -> Az1 -> Az11)
        adjusted_line = regexes
            .type_pattern
            .replace_all(&adjusted_line, format!("{}$1", prefix))
            .to_string();

        // Transform paths from crate:: (final azul crate) to super:: (memtest module context)
        // In generated.rs we have: mod dll { }, pub mod vec { }, etc.
        // So crate::dll:: needs to become super::dll:: when inside pub mod vec
        adjusted_line = adjusted_line.replace("crate::dll::", "super::dll::");
        adjusted_line = adjusted_line.replace("crate::vec::", "super::vec::");
        // Special case: crate::str::String needs to become super::str::{prefix}AzString
        // because AzString is in api.json and gets prefixed
        adjusted_line = adjusted_line.replace("crate::str::String", &format!("super::str::{}AzString", prefix));
        adjusted_line = adjusted_line.replace("crate::str::", "super::str::");
        adjusted_line = adjusted_line.replace("crate::option::", "super::option::");
        adjusted_line = adjusted_line.replace("crate::dom::", "super::dom::");
        adjusted_line = adjusted_line.replace("crate::gl::", "super::gl::");
        adjusted_line = adjusted_line.replace("crate::css::", "super::css::");
        adjusted_line = adjusted_line.replace("crate::window::", "super::window::");
        adjusted_line = adjusted_line.replace("crate::callbacks::", "super::callbacks::");
        adjusted_line = adjusted_line.replace("crate::prelude::", "");

        output.push_str(&adjusted_line);
        output.push('\n');
    }

    println!("      [OK] Processed {} lines total", line_count);
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
