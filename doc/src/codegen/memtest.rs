// Code generation for memory layout tests
// This module generates a complete test crate that validates memory layouts

use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
};

use indexmap::IndexMap;

use super::{
    func_gen::{build_functions_map, build_functions_map_ext},
    struct_gen::{generate_structs, GenerateConfig, StructMetadata},
};
use crate::api::*;

pub type Result<T> = std::result::Result<T, String>;

/// Type replacement data used throughout memtest generation
#[derive(Debug)]
pub struct TypeReplacements {
    /// Set of all type names that need prefixing
    pub type_names: HashSet<String>,
}

impl TypeReplacements {
    pub fn new(version_data: &VersionData) -> Result<Self> {
        println!("      [BUILD] Collecting type names for replacement...");

        // Collect all type names from api.json
        let mut type_names = HashSet::new();
        for module_data in version_data.api.values() {
            for class_name in module_data.classes.keys() {
                // Skip special cases and primitive types
                if class_name == "String" || class_name == "Vec" {
                    continue;
                }
                // Skip primitive types that should never get a prefix
                if PRIMITIVE_TYPES.contains(&class_name.as_str()) {
                    continue;
                }
                // Skip single-letter generic type parameters (e.g., T, U, V)
                if is_generic_type_param(class_name) {
                    continue;
                }
                type_names.insert(class_name.clone());
            }
        }

        println!("      [OK] Collected {} type names", type_names.len());

        Ok(Self { type_names })
    }

    /// Replace all occurrences of a type name with word boundary matching
    pub fn replace_type_in_line(&self, line: &str, type_name: &str, replacement: &str) -> String {
        replace_word_boundary(line, type_name, replacement)
    }

    /// Replace all Az-prefixed types with versioned prefix
    pub fn replace_az_types(&self, line: &str, prefix: &str) -> String {
        replace_az_prefix(line, prefix)
    }
}

/// Replace a word with word-boundary matching (no regex)
fn replace_word_boundary(input: &str, word: &str, replacement: &str) -> String {
    if word.is_empty() || !input.contains(word) {
        return input.to_string();
    }

    let mut result = String::with_capacity(input.len() + 64);
    let mut remaining = input;

    while let Some(pos) = remaining.find(word) {
        // Check if it's a word boundary
        let before_ok = pos == 0 || !is_word_char(remaining.as_bytes()[pos - 1]);
        let after_pos = pos + word.len();
        let after_ok =
            after_pos >= remaining.len() || !is_word_char(remaining.as_bytes()[after_pos]);

        if before_ok && after_ok {
            result.push_str(&remaining[..pos]);
            result.push_str(replacement);
            remaining = &remaining[after_pos..];
        } else {
            result.push_str(&remaining[..pos + word.len()]);
            remaining = &remaining[pos + word.len()..];
        }
    }
    result.push_str(remaining);
    result
}

/// Check if a byte is a word character (alphanumeric or underscore)
fn is_word_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Replace Az-prefixed types with versioned prefix (e.g., AzFoo -> Az1Foo)
fn replace_az_prefix(input: &str, prefix: &str) -> String {
    let mut result = String::with_capacity(input.len() + 64);
    let bytes = input.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        // Look for "Az" followed by uppercase letter
        if i + 2 < bytes.len()
            && bytes[i] == b'A'
            && bytes[i + 1] == b'z'
            && bytes[i + 2].is_ascii_uppercase()
        {
            // Check word boundary before "Az"
            let before_ok = i == 0 || !is_word_char(bytes[i - 1]);

            if before_ok {
                // Find the end of the identifier
                let start = i + 2;
                let mut end = start;
                while end < bytes.len()
                    && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_')
                {
                    end += 1;
                }

                // Replace Az with the prefix
                result.push_str(prefix);
                result.push_str(&input[start..end]);
                i = end;
                continue;
            }
        }

        result.push(bytes[i] as char);
        i += 1;
    }

    result
}

/// Configuration for memtest generation
#[derive(Debug, Clone)]
pub struct MemtestConfig {
    /// Remove serde-support feature gates
    pub remove_serde: bool,
    /// Remove other optional features
    pub remove_optional_features: Vec<String>,
    /// Generate actual fn_body from api.json instead of unimplemented!() stubs
    /// When true, functions will contain the implementation from api.json
    /// When false, functions will have unimplemented!() stubs (for quick testing)
    pub generate_fn_bodies: bool,
    /// Whether we're generating for DLL include!() (true) or memtest crate (false)
    /// When true: azul_dll:: is replaced with crate:: (compiling inside azul-dll)
    /// When false: azul_dll:: stays as is (memtest uses azul_dll as dependency)
    pub is_for_dll: bool,
    /// Whether to generate #[no_mangle] on C-ABI functions
    /// When false, functions can be duplicated without symbol conflicts
    pub generate_no_mangle: bool,
    /// Whether to skip generating C-ABI functions entirely
    /// When true, only structs and enums are generated (for Python extension)
    pub skip_c_abi_functions: bool,
    /// Whether to generate Clone/Drop by transmuting to external type
    /// (for Python extension where C-ABI functions don't exist)
    pub drop_via_external: bool,
    /// Whether to generate callback_typedef types as aliases to external types
    /// (e.g., `pub type AzLayoutCallbackType = azul_core::callbacks::LayoutCallbackType;`)
    pub callback_typedef_use_external: bool,
    /// Whether to generate extern "C" { } declarations instead of function definitions
    /// When true, functions are declared as `extern "C" { fn AzFoo(...) -> ...; }`
    /// When false (default), functions are defined with bodies
    /// Used for dynamic linking mode where the DLL is loaded at runtime
    pub extern_declarations_only: bool,
    /// Name of the library to link against (for dynamic linking)
    /// When set, generates `#[link(name = "...")]` attribute
    pub link_library_name: Option<String>,
}

impl Default for MemtestConfig {
    fn default() -> Self {
        Self {
            remove_serde: false, // Keep serde lines, we add serde deps to Cargo.toml
            remove_optional_features: vec![],
            generate_fn_bodies: false, // Disabled by default - api.json fn_body needs cleanup
            is_for_dll: false,         // Default is memtest mode
            generate_no_mangle: true,  // Default is to generate #[no_mangle]
            skip_c_abi_functions: false, // Default is to generate C-ABI functions
            drop_via_external: false,  // Default is to call C-ABI functions for Drop/Clone
            callback_typedef_use_external: false, // Default is to re-define callback function signatures
            extern_declarations_only: false, // Default is to generate function definitions with bodies
            link_library_name: None,   // No link attribute by default
        }
    }
}

/// Generate ONLY the API definitions (structs, enums, functions) without tests
/// This is used for the DLL include!() macro
pub fn generate_dll_api(api_data: &ApiData, project_root: &Path) -> Result<()> {
    println!("  [DLL] Generating API definitions for DLL...");

    let mut config = MemtestConfig::default();
    config.generate_fn_bodies = true; // Enable real function bodies for DLL
    config.is_for_dll = true; // This is for DLL include!(), not memtest crate

    let output_path = project_root
        .join("target")
        .join("memtest")
        .join("dll_api.rs");

    // Get version data
    let version_name = api_data
        .0
        .keys()
        .next()
        .ok_or_else(|| "No version name found".to_string())?;
    let version_data = api_data
        .get_version(version_name)
        .ok_or_else(|| "No API version found".to_string())?;

    // Collect type names for replacement
    let replacements = TypeReplacements::new(version_data)?;

    // Create output directory
    fs::create_dir_all(output_path.parent().unwrap())
        .map_err(|e| format!("Failed to create output dir: {}", e))?;

    // Generate only the dll module content (without the test lib.rs)
    let dll_content = generate_generated_rs(api_data, &config, &replacements)?;

    println!(
        "  [SAVE] Writing dll_api.rs ({} bytes)...",
        dll_content.len()
    );
    fs::write(&output_path, dll_content)
        .map_err(|e| format!("Failed to write dll_api.rs: {}", e))?;

    println!("[OK] Generated DLL API at: {}", output_path.display());
    println!("\nTo use in dll/src/lib.rs:");
    println!(
        "  include!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/../target/memtest/dll_api.rs\"));"
    );

    Ok(())
}

/// Generate API definitions for DYNAMIC linking (only declarations, no function bodies)
/// This is used when the Rust code links against a pre-compiled .dylib/.so/.dll
/// The generated file contains:
/// - All struct/enum type definitions (same as static linking)
/// - extern "C" { } blocks with function declarations (no bodies)
/// - #[link(name = "azul_dll")] attribute to link against the DLL
pub fn generate_dll_api_dynamic(api_data: &ApiData, project_root: &Path) -> Result<()> {
    println!("  [DLL-DYNAMIC] Generating API definitions for dynamic linking...");

    let mut config = MemtestConfig::default();
    config.is_for_dll = true;
    config.extern_declarations_only = true; // Generate extern declarations, not definitions
    config.link_library_name = Some("azul_dll".to_string()); // Link against libazul_dll.dylib
    config.generate_no_mangle = false; // No need for #[no_mangle] on declarations
    config.skip_c_abi_functions = false; // We need the function declarations
    // CRITICAL: For dynamic linking, we cannot use transmute to azul_core types!
    // Instead, Clone/Drop implementations should call the DLL functions.
    config.drop_via_external = false; // Don't use transmute to external types
    // callback_typedef_use_external = false means we define our own callback types

    let output_path = project_root
        .join("target")
        .join("codegen")
        .join("dll_api_dynamic.rs");

    // Get version data
    let version_name = api_data
        .0
        .keys()
        .next()
        .ok_or_else(|| "No version name found".to_string())?;
    let version_data = api_data
        .get_version(version_name)
        .ok_or_else(|| "No API version found".to_string())?;

    // Collect type names for replacement
    let replacements = TypeReplacements::new(version_data)?;

    // Create output directory
    fs::create_dir_all(output_path.parent().unwrap())
        .map_err(|e| format!("Failed to create output dir: {}", e))?;

    // Generate the dynamic linking API
    let dll_content = generate_generated_rs(api_data, &config, &replacements)?;

    println!(
        "  [SAVE] Writing dll_api_dynamic.rs ({} bytes)...",
        dll_content.len()
    );
    fs::write(&output_path, dll_content)
        .map_err(|e| format!("Failed to write dll_api_dynamic.rs: {}", e))?;

    println!("[OK] Generated dynamic DLL API at: {}", output_path.display());

    Ok(())
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

    // Collect type names for replacement
    println!("  [BUILD] Collecting type replacement data...");
    let replacements = TypeReplacements::new(version_data)?;

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
    let generated_rs = generate_generated_rs(api_data, &config, &replacements)?;
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
azul-layout = { path = "../../layout", features = ["widgets", "extra"] }
azul-css = { path = "../../css" }
azul-dll = { path = "../../dll" }

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
    output.push_str("#![allow(unused_variables)]\n");
    output.push_str("#![deny(improper_ctypes_definitions)]\n\n");

    output.push_str("use std::mem;\n\n");
    output.push_str("pub mod generated;\n\n");

    // Collect all test cases
    let mut test_cases = Vec::new();

    // Valid external crate prefixes that we can test against
    // Note: azul_dll is NOT included - memtest runs before C API generation
    let valid_crate_prefixes = ["azul_core::", "azul_css::", "azul_layout::"];

    for (version_name, version_data) in &api_data.0 {
        for (module_name, module_data) in &version_data.api {
            for (class_name, class_data) in &module_data.classes {
                if let Some(external_path) = &class_data.external {
                    // Skip generic types - they can't be tested without concrete type parameters
                    let is_generic = class_data.generic_params.is_some();

                    // Skip types from crates we don't have as dependencies
                    let has_valid_crate = valid_crate_prefixes
                        .iter()
                        .any(|prefix| external_path.starts_with(prefix));
                    if !has_valid_crate {
                        continue;
                    }

                    test_cases.push(TestCase {
                        version: version_name.clone(),
                        module: module_name.clone(),
                        class: class_name.clone(),
                        external_path: external_path.clone(),
                        has_struct: class_data.struct_fields.is_some(),
                        has_enum: class_data.enum_fields.is_some(),
                        enum_fields: class_data.enum_fields.clone(),
                        is_generic,
                    });
                }
            }
        }
    }

    output.push_str(&format!("// Found {} types to test\n\n", test_cases.len()));

    // Generate test for each type
    for test_case in &test_cases {
        // Skip generic types - they require concrete type parameters
        if test_case.is_generic {
            continue;
        }

        // Skip types without struct_fields or enum_fields - they aren't generated in generated.rs
        if !test_case.has_struct && !test_case.has_enum {
            continue;
        }

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
    is_generic: bool,
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
    let generated_type = format!("crate::generated::dll::Az{}", test.class);
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
    let generated_type = format!("crate::generated::dll::Az{}", test.class);
    let external_type = &test.external_path;

    output.push_str(&format!("/// Test discriminant order of {}\n", test.class));
    output.push_str(&format!("#[test]\n"));
    output.push_str(&format!("fn {}() {{\n", test_name));

    // Collect all variant names in order
    let variants: Vec<(String, bool)> = enum_fields
        .iter()
        .filter_map(|variant_map| {
            variant_map
                .iter()
                .next()
                .map(|(name, data)| (name.clone(), data.r#type.is_some()))
        })
        .collect();

    if variants.is_empty() {
        output.push_str("    // No variants to test\n");
        output.push_str("}\n");
        return Ok(output);
    }

    // Generate instances for each variant - use actual variant constructors
    // We create the external type and transmute it to the generated type,
    // then compare discriminants to ensure they match
    for (idx, (variant_name, has_data)) in variants.iter().enumerate() {
        if *has_data {
            // Skip variants with data for now - they need more complex handling
            output.push_str(&format!(
                "    // Variant {} ({}) has data - skipping discriminant test\n",
                idx, variant_name
            ));
        } else {
            // For variants without data, create external type and transmute to generated
            output.push_str(&format!(
                "    let external_{} = {}::{};\n",
                idx, external_type, variant_name
            ));
            output.push_str(&format!(
                "    let generated_{} = {}::{};\n",
                idx, generated_type, variant_name
            ));
            output.push_str(&format!(
                "    let transmuted_{}: {} = unsafe {{ std::mem::transmute(external_{}) }};\n",
                idx, generated_type, idx
            ));
            output.push_str(&format!(
                "    let gen_disc_{} = std::mem::discriminant(&generated_{});\n",
                idx, idx
            ));
            output.push_str(&format!(
                "    let transmuted_disc_{} = std::mem::discriminant(&transmuted_{});\n",
                idx, idx
            ));
        }
    }

    output.push_str("\n");
    output
        .push_str("    // Verify discriminants match between generated and transmuted external\n");

    // For variants without data, compare generated vs transmuted external discriminants
    for (idx, (variant_name, has_data)) in variants.iter().enumerate() {
        if !*has_data {
            output.push_str(&format!(
                "    assert_eq!(gen_disc_{}, transmuted_disc_{}, \"Discriminant mismatch for \
                 variant {}: external type has different discriminant value\");\n",
                idx, idx, variant_name
            ));
        }
    }

    output.push_str("\n");
    output.push_str("    // Verify all discriminants are unique within generated type\n");

    // Compare discriminants to ensure they're all different (only for variants without data)
    let no_data_indices: Vec<usize> = variants
        .iter()
        .enumerate()
        .filter(|(_, (_, has_data))| !*has_data)
        .map(|(idx, _)| idx)
        .collect();

    for i in 0..no_data_indices.len() {
        for j in (i + 1)..no_data_indices.len() {
            let idx_i = no_data_indices[i];
            let idx_j = no_data_indices[j];
            let name_i = &variants[idx_i].0;
            let name_j = &variants[idx_j].0;
            output.push_str(&format!(
                "    assert_ne!(gen_disc_{}, gen_disc_{}, \"{} should != {}\");\n",
                idx_i, idx_j, name_i, name_j
            ));
        }
    }

    output.push_str("}\n");

    Ok(output)
}

fn sanitize_name(name: &str) -> String {
    name.replace(".", "_")
        .replace("-", "_")
        .replace("::", "_")
        .to_lowercase()
}

/// Build a map from Az-prefixed type name to external Rust type path
/// This is used for transmute operations between C-API types and internal Rust types
pub fn build_type_to_external_map(
    version_data: &VersionData,
    prefix: &str,
    is_for_dll: bool,
) -> std::collections::HashMap<String, String> {
    let mut type_to_external: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for module_data in version_data.api.values() {
        for (class_name, class_data) in &module_data.classes {
            if let Some(external) = &class_data.external {
                let prefixed_name = format!("{}{}", prefix, class_name);
                // For DLL mode: Replace azul_dll:: with crate:: since the generated code runs
                // inside azul-dll For memtest mode: Keep azul_dll:: as is since
                // memtest uses azul_dll as a dependency
                let external_fixed = if is_for_dll {
                    external.replace("azul_dll::", "crate::")
                } else {
                    external.clone()
                };
                type_to_external.insert(prefixed_name, external_fixed);
            }
        }
    }
    type_to_external
}

pub fn generate_generated_rs(
    api_data: &ApiData,
    config: &MemtestConfig,
    replacements: &TypeReplacements,
) -> Result<String> {
    println!("    [WAIT] Starting generated.rs creation...");
    let mut output = String::new();

    output.push_str("// Auto-generated API definitions from api.json for memtest\n");
    // Note: Using #[allow] instead of #![allow] for include!() compatibility
    output.push_str(
        "#[allow(dead_code, unused_imports, non_camel_case_types, non_snake_case, unused_unsafe, \
         clippy::all)]\n",
    );
    output.push_str("#[deny(improper_ctypes_definitions)]\n");
    // Make the inner module public so it can be re-exported from lib.rs
    output.push_str("pub mod __dll_api_inner {\n\n");
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
        replacements,
    )?);

    println!("    [PKG] Generating public API modules...");
    // 2. Generate the public API modules (`pub mod str`, etc.)
    output.push_str(&generate_public_api_modules(
        version_data,
        &prefix,
        config,
        replacements,
    )?);

    // Close the __dll_api_inner module
    output.push_str("}\n\n");
    output.push_str("pub use __dll_api_inner::*;\n");

    println!("    [OK] Generated.rs creation complete");
    Ok(output)
}

fn generate_dll_module(
    version_data: &VersionData,
    prefix: &str,
    config: &MemtestConfig,
    replacements: &TypeReplacements,
) -> Result<String> {
    println!("      [BUILD]  Building dll module...");
    let mut dll_code = String::new();
    dll_code.push_str("pub mod dll {\n");
    dll_code.push_str("    use super::c_void;\n");
    dll_code.push_str("    use std::{string, vec, slice, mem, fmt, cmp, hash, iter};\n");
    dll_code.push_str("    use std::sync::atomic::AtomicUsize;\n");
    dll_code.push_str("    use std::sync::Arc;\n\n");

    // Add GL type aliases (from gl_context_loader crate)
    dll_code.push_str("    // ===== GL Type Aliases =====\n");
    dll_code.push_str("    pub type GLenum = u32;\n");
    dll_code.push_str("    pub type GLboolean = u8;\n");
    dll_code.push_str("    pub type GLbitfield = u32;\n");
    dll_code.push_str("    pub type GLvoid = c_void;\n");
    dll_code.push_str("    pub type GLbyte = i8;\n");
    dll_code.push_str("    pub type GLshort = i16;\n");
    dll_code.push_str("    pub type GLint = i32;\n");
    dll_code.push_str("    pub type GLclampx = i32;\n");
    dll_code.push_str("    pub type GLubyte = u8;\n");
    dll_code.push_str("    pub type GLushort = u16;\n");
    dll_code.push_str("    pub type GLuint = u32;\n");
    dll_code.push_str("    pub type GLsizei = i32;\n");
    dll_code.push_str("    pub type GLfloat = f32;\n");
    dll_code.push_str("    pub type GLclampf = f32;\n");
    dll_code.push_str("    pub type GLdouble = f64;\n");
    dll_code.push_str("    pub type GLclampd = f64;\n");
    dll_code.push_str("    pub type GLeglImageOES = *const c_void;\n");
    dll_code.push_str("    pub type GLchar = i8;\n");
    dll_code.push_str("    pub type GLcharARB = i8;\n");
    dll_code.push_str("    pub type GLhandleARB = u32;\n");
    dll_code.push_str("    pub type GLhalfARB = u16;\n");
    dll_code.push_str("    pub type GLhalf = u16;\n");
    dll_code.push_str("    pub type GLfixed = i32;\n");
    dll_code.push_str("    pub type GLintptr = isize;\n");
    dll_code.push_str("    pub type GLsizeiptr = isize;\n");
    dll_code.push_str("    pub type GLint64 = i64;\n");
    dll_code.push_str("    pub type GLuint64 = u64;\n");
    dll_code.push_str("    pub type GLintptrARB = isize;\n");
    dll_code.push_str("    pub type GLsizeiptrARB = isize;\n");
    dll_code.push_str("    pub type GLint64EXT = i64;\n");
    dll_code.push_str("    pub type GLuint64EXT = u64;\n");
    dll_code.push_str("    pub type GLhalfNV = u16;\n");
    dll_code.push_str("    pub type GLvdpauSurfaceNV = isize;\n\n");

    // Add missing type aliases for primitives used in api.json generics
    dll_code.push_str("    // ===== Primitive Type Aliases (for generic instantiations) =====\n");
    dll_code.push_str(&format!("    pub type {}I32 = i32;\n", prefix));
    dll_code.push_str(&format!("    pub type {}U32 = u32;\n", prefix));
    dll_code.push_str(&format!("    pub type {}F32 = f32;\n", prefix));
    dll_code.push_str(&format!("    pub type {}Usize = usize;\n", prefix));
    dll_code.push_str(&format!("    pub type {}C_void = c_void;\n", prefix));
    // Non-prefixed aliases for primitive types
    dll_code.push_str("    pub type Usize = usize;\n");
    dll_code.push_str("    pub type U8 = u8;\n");
    dll_code.push_str("    pub type I16 = i16;\n");
    dll_code.push_str("    pub type Char = char;\n");
    // Note: Option<T> types must use AzOptionT repr(C) structs, not std::Option
    dll_code.push_str("\n");

    println!("      [STATS] Collecting structs...");
    // Collect all structs for this version
    // Use entry API to prefer versions with struct_fields/enum_fields over empty external
    // references
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
        private_pointers: false,
        no_derive: false,
        wrapper_postfix: String::new(),
        // For extern_declarations_only (dynamic linking): use_extern_clone_drop=true so Clone/Drop
        // call the extern "C" _deepCopy/_delete functions. For static linking: use drop_via_external.
        is_memtest: if config.extern_declarations_only { false } else { !config.drop_via_external },
        is_for_dll: config.is_for_dll,
        drop_via_external: if config.extern_declarations_only { false } else { config.drop_via_external },
        callback_typedef_use_external: config.callback_typedef_use_external,
        skip_external_trait_impls: false,
        use_extern_clone_drop: config.extern_declarations_only, // For dynamic linking
    };
    dll_code.push_str(
        &generate_structs(version_data, &structs_map, &struct_config).map_err(|e| e.to_string())?,
    );

    // Generate Debug/PartialEq/PartialOrd implementations for VecDestructor types
    // These contain function pointers which can't derive these traits, so we compare by pointer
    // address
    println!("      [IMPL] Generating VecDestructor trait implementations...");
    dll_code.push_str("\n    // ===== VecDestructor Trait Implementations =====\n");
    dll_code.push_str("    // Function pointers compared by address as usize\n\n");

    for prefixed_name in structs_map.keys() {
        // Keys are already prefixed like "AzU8VecDestructor"
        if prefixed_name.ends_with("VecDestructor") && !prefixed_name.ends_with("VecDestructorType")
        {
            // Debug implementation
            dll_code.push_str(&format!(
                r#"    impl core::fmt::Debug for {name} {{
        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {{
            match self {{
                {name}::DefaultRust => write!(f, "{name}::DefaultRust"),
                {name}::NoDestructor => write!(f, "{name}::NoDestructor"),
                {name}::External(fn_ptr) => write!(f, "{name}::External({{:p}})", *fn_ptr as *const ()),
            }}
        }}
    }}

"#, name = prefixed_name));

            // PartialEq implementation
            dll_code.push_str(&format!(
                r#"    impl PartialEq for {name} {{
        fn eq(&self, other: &Self) -> bool {{
            match (self, other) {{
                ({name}::DefaultRust, {name}::DefaultRust) => true,
                ({name}::NoDestructor, {name}::NoDestructor) => true,
                ({name}::External(a), {name}::External(b)) => (*a as usize) == (*b as usize),
                _ => false,
            }}
        }}
    }}

"#,
                name = prefixed_name
            ));

            // PartialOrd implementation
            dll_code.push_str(&format!(
                r#"    impl PartialOrd for {name} {{
        fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {{
            Some(self.cmp(other))
        }}
    }}

"#,
                name = prefixed_name
            ));

            // Eq implementation (since PartialEq is implemented)
            dll_code.push_str(&format!(
                r#"    impl Eq for {name} {{ }}

"#,
                name = prefixed_name
            ));

            // Ord implementation
            dll_code.push_str(&format!(
                r#"    impl Ord for {name} {{
        fn cmp(&self, other: &Self) -> core::cmp::Ordering {{
            let self_ord = match self {{
                {name}::DefaultRust => 0usize,
                {name}::NoDestructor => 1usize,
                {name}::External(f) => 2usize + (*f as usize),
            }};
            let other_ord = match other {{
                {name}::DefaultRust => 0usize,
                {name}::NoDestructor => 1usize,
                {name}::External(f) => 2usize + (*f as usize),
            }};
            self_ord.cmp(&other_ord)
        }}
    }}

"#,
                name = prefixed_name
            ));

            // Hash implementation
            dll_code.push_str(&format!(
                r#"    impl core::hash::Hash for {name} {{
        fn hash<H: core::hash::Hasher>(&self, state: &mut H) {{
            match self {{
                {name}::DefaultRust => 0usize.hash(state),
                {name}::NoDestructor => 1usize.hash(state),
                {name}::External(f) => (2usize + (*f as usize)).hash(state),
            }}
        }}
    }}

"#,
                name = prefixed_name
            ));

            // Clone implementation - function pointers are Copy, so cloning is trivial
            dll_code.push_str(&format!(
                r#"    impl Clone for {name} {{
        fn clone(&self) -> Self {{
            match self {{
                {name}::DefaultRust => {name}::DefaultRust,
                {name}::NoDestructor => {name}::NoDestructor,
                {name}::External(f) => {name}::External(*f),
            }}
        }}
    }}

"#,
                name = prefixed_name
            ));
        }
    }

    // NOTE: FontRef trait implementations are now generated by struct_gen.rs
    // based on custom_impls in api.json. No need to generate them here.

    // Generate VecRef as_slice/as_mut_slice methods and From implementations
    // Based on vec_ref_element_type field in api.json
    println!("      [IMPL] Generating VecRef slice methods...");
    dll_code.push_str("\n    // ===== VecRef Slice Methods =====\n\n");

    for (prefixed_name, struct_meta) in &structs_map {
        if let Some(element_type) = &struct_meta.vec_ref_element_type {
            let is_mut = struct_meta.vec_ref_is_mut;
            let unprefixed_name = prefixed_name.strip_prefix(prefix).unwrap_or(prefixed_name);

            // Determine the element type with prefix if it's a custom type
            let prefixed_element = if PRIMITIVE_TYPES.contains(&element_type.as_str()) {
                element_type.clone()
            } else {
                format!("{}{}", prefix, element_type)
            };

            if is_mut {
                // Mutable VecRef: as_slice and as_mut_slice
                dll_code.push_str(&format!(
                    r#"    impl {name} {{
        pub fn as_slice(&self) -> &[{elem}] {{
            unsafe {{ core::slice::from_raw_parts(self.ptr, self.len) }}
        }}
        pub fn as_mut_slice(&mut self) -> &mut [{elem}] {{
            unsafe {{ core::slice::from_raw_parts_mut(self.ptr, self.len) }}
        }}
    }}

    impl<'a> From<&'a mut [{elem}]> for {name} {{
        fn from(s: &'a mut [{elem}]) -> Self {{
            Self {{ ptr: s.as_mut_ptr(), len: s.len() }}
        }}
    }}

"#,
                    name = prefixed_name,
                    elem = prefixed_element
                ));
            } else {
                // Immutable VecRef: only as_slice
                // Special case for Refstr (which is &str, not a slice)
                if unprefixed_name == "Refstr" {
                    dll_code.push_str(&format!(
                        r#"    impl {name} {{
        pub fn as_str(&self) -> &str {{
            unsafe {{ core::str::from_utf8_unchecked(core::slice::from_raw_parts(self.ptr, self.len)) }}
        }}
    }}

    impl<'a> From<&'a str> for {name} {{
        fn from(s: &'a str) -> Self {{
            Self {{ ptr: s.as_ptr(), len: s.len() }}
        }}
    }}

"#, name = prefixed_name));
                } else {
                    dll_code.push_str(&format!(
                        r#"    impl {name} {{
        pub fn as_slice(&self) -> &[{elem}] {{
            unsafe {{ core::slice::from_raw_parts(self.ptr, self.len) }}
        }}
    }}

    impl<'a> From<&'a [{elem}]> for {name} {{
        fn from(s: &'a [{elem}]) -> Self {{
            Self {{ ptr: s.as_ptr(), len: s.len() }}
        }}
    }}

"#,
                        name = prefixed_name,
                        elem = prefixed_element
                    ));
                }
            }
        }
    }

    // Generate trait implementations for VecRef types
    // These are slice wrappers that can derive traits based on their element type
    println!("      [IMPL] Generating VecRef trait implementations...");
    dll_code.push_str("\n    // ===== VecRef Trait Implementations =====\n\n");

    // Types that don't implement Ord/Hash (floating point types)
    let no_ord_hash_types = ["f32", "f64"];

    for (prefixed_name, struct_meta) in &structs_map {
        if let Some(element_type) = &struct_meta.vec_ref_element_type {
            let is_mut = struct_meta.vec_ref_is_mut;
            let unprefixed_name = prefixed_name.strip_prefix(prefix).unwrap_or(prefixed_name);

            // Refstr uses as_str() instead of as_slice()
            let slice_method = if unprefixed_name == "Refstr" {
                "as_str"
            } else {
                "as_slice"
            };

            // Check if element type supports Ord/Hash
            let supports_ord_hash = !no_ord_hash_types.contains(&element_type.as_str());

            // Determine the element type with prefix if it's a custom type
            let prefixed_element = if PRIMITIVE_TYPES.contains(&element_type.as_str()) {
                element_type.clone()
            } else {
                format!("{}{}", prefix, element_type)
            };

            // Debug implementation
            dll_code.push_str(&format!(
                r#"    impl core::fmt::Debug for {name} {{
        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {{
            self.{method}().fmt(f)
        }}
    }}

"#,
                name = prefixed_name,
                method = slice_method
            ));

            // Clone implementation (creates a new reference to same data)
            dll_code.push_str(&format!(
                r#"    impl Clone for {name} {{
        fn clone(&self) -> Self {{
            Self {{ ptr: self.ptr, len: self.len }}
        }}
    }}

"#,
                name = prefixed_name
            ));

            // Copy implementation (VecRef is just a fat pointer, so it's Copy)
            dll_code.push_str(&format!(
                r#"    impl Copy for {name} {{}}

"#,
                name = prefixed_name
            ));

            // PartialEq implementation (compare slices)
            dll_code.push_str(&format!(
                r#"    impl PartialEq for {name} {{
        fn eq(&self, other: &Self) -> bool {{
            self.{method}() == other.{method}()
        }}
    }}

"#,
                name = prefixed_name,
                method = slice_method
            ));

            // PartialOrd implementation (always available)
            dll_code.push_str(&format!(
                r#"    impl PartialOrd for {name} {{
        fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {{
            self.{method}().partial_cmp(other.{method}())
        }}
    }}

"#,
                name = prefixed_name,
                method = slice_method
            ));

            // Eq, Ord and Hash only for types that support it (not f32/f64)
            if supports_ord_hash {
                // Eq implementation (f32/f64 don't implement Eq due to NaN)
                dll_code.push_str(&format!(
                    r#"    impl Eq for {name} {{}}

"#,
                    name = prefixed_name
                ));

                // Ord implementation
                dll_code.push_str(&format!(
                    r#"    impl Ord for {name} {{
        fn cmp(&self, other: &Self) -> core::cmp::Ordering {{
            self.{method}().cmp(other.{method}())
        }}
    }}

"#,
                    name = prefixed_name,
                    method = slice_method
                ));

                // Hash implementation
                dll_code.push_str(&format!(
                    r#"    impl core::hash::Hash for {name} {{
        fn hash<H: core::hash::Hasher>(&self, state: &mut H) {{
            self.{method}().hash(state)
        }}
    }}

"#,
                    name = prefixed_name,
                    method = slice_method
                ));
            }
        }
    }

    // Generate as_str() method for String type (wrapper around U8Vec)
    // NOTE: This is currently disabled because U8Vec in api.json only has 'ptr' field,
    // missing the 'len' field needed for as_str(). The real String type works because
    // U8Vec in the actual source code has all fields.
    // TODO: Fix U8Vec in api.json to include all fields (ptr, len, cap, destructor)
    println!("      [IMPL] Skipping String as_str() method (U8Vec incomplete in api.json)...");
    dll_code.push_str("\n    // ===== String Methods =====\n");
    dll_code.push_str("    // NOTE: as_str() not generated - U8Vec in api.json is incomplete\n\n");

    // NOTE: Callback trait implementations are now auto-generated by generate_structs
    // when a struct has exactly one field whose type is a callback_typedef

    // Build a map from prefixed type name to external path for custom_impl types
    let mut type_to_external: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for module_data in version_data.api.values() {
        for (class_name, class_data) in &module_data.classes {
            if let Some(external) = &class_data.external {
                let prefixed_name = format!("{}{}", prefix, class_name);
                // For DLL mode: Replace azul_dll:: with crate:: since the generated code runs
                // inside azul-dll For memtest mode: Keep azul_dll:: as is since
                // memtest uses azul_dll as a dependency
                let external_fixed = if config.is_for_dll {
                    external.replace("azul_dll::", "crate::")
                } else {
                    external.clone()
                };
                type_to_external.insert(prefixed_name, external_fixed);
            }
        }
    }

    // Skip C-ABI function generation if configured (for Python extension)
    if config.skip_c_abi_functions {
        println!("      [SKIP] Skipping C-ABI functions (skip_c_abi_functions=true)");
        dll_code.push_str("\n    // C-ABI functions skipped (skip_c_abi_functions=true)\n");
    } else if config.extern_declarations_only {
        // Generate extern "C" { } block with function declarations (for dynamic linking)
        println!("      [TARGET] Generating extern function declarations (dynamic linking)...");
        let functions_map_ext =
            build_functions_map_ext(version_data, prefix).map_err(|e| e.to_string())?;
        println!("      [LINK] Found {} functions", functions_map_ext.len());
        
        // Add #[link] attribute if library name is specified
        if let Some(lib_name) = &config.link_library_name {
            dll_code.push_str(&format!("\n    #[link(name = \"{}\")]\n", lib_name));
        }
        
        dll_code.push_str("    extern \"C\" {\n");
        
        for (fn_name, fn_info) in &functions_map_ext {
            let return_str = if fn_info.return_type.is_empty() {
                "".to_string()
            } else {
                format!(" -> {}", fn_info.return_type)
            };
            
            // Generate function declaration (no body)
            dll_code.push_str(&format!(
                "        pub fn {}({}){};\n",
                fn_name, fn_info.fn_args, return_str
            ));
        }
        
        dll_code.push_str("    }\n");
        dll_code.push_str("    // --- End extern \"C\" declarations ---\n\n");
    } else {
        println!("      [TARGET] Generating function bodies...");
        // Generate function implementations from api.json fn_body
        // All functions MUST have fn_body defined - missing fn_body will cause an error
        let functions_map_ext =
            build_functions_map_ext(version_data, prefix).map_err(|e| e.to_string())?;
        println!("      [LINK] Found {} functions", functions_map_ext.len());
        dll_code.push_str("\n    // --- C-ABI Functions ---\n");

        for (fn_name, fn_info) in &functions_map_ext {
            let return_str = if fn_info.return_type.is_empty() {
                "".to_string()
            } else {
                format!(" -> {}", fn_info.return_type)
            };

            // Determine the function body
            let fn_body = if fn_name.ends_with("_deepCopy") {
                // Extract type name from function name: AzTypeName_deepCopy -> AzTypeName
                let type_name = fn_name.strip_suffix("_deepCopy").unwrap_or(fn_name);
                if let Some(external_path) = type_to_external.get(type_name) {
                    // Cast to external type, clone, cast back
                    format!(
                        "core::mem::transmute::<{ext}, {local}>((*(object as *const {local} as *const \
                         {ext})).clone())",
                        ext = external_path,
                        local = type_name
                    )
                } else {
                    // Fallback: just call clone directly (may fail if Clone not derived)
                    "object.clone()".to_string()
                }
            } else if fn_name.ends_with("_delete") {
                // Extract type name from function name: AzTypeName_delete -> AzTypeName
                let type_name = fn_name.strip_suffix("_delete").unwrap_or(fn_name);
                if let Some(external_path) = type_to_external.get(type_name) {
                    // Cast to external type and drop
                    format!(
                        "core::ptr::drop_in_place(object as *mut {local} as *mut {ext})",
                        ext = external_path,
                        local = type_name
                    )
                } else {
                    // Fallback: just drop directly
                    "core::ptr::drop_in_place(object)".to_string()
                }
            } else if fn_name.ends_with("_partialEq") {
                // Extract type name from function name: AzTypeName_partialEq -> AzTypeName
                let type_name = fn_name.strip_suffix("_partialEq").unwrap_or(fn_name);
                if let Some(external_path) = type_to_external.get(type_name) {
                    // Cast to external type and compare
                    format!(
                        "(*(a as *const {local} as *const {ext})) == (*(b as *const {local} as *const {ext}))",
                        ext = external_path,
                        local = type_name
                    )
                } else {
                    // Fallback: compare directly
                    "*a == *b".to_string()
                }
            } else if fn_name.ends_with("_partialCmp") {
                // Extract type name from function name: AzTypeName_partialCmp -> AzTypeName
                // Returns: 0 = Less, 1 = Equal, 2 = Greater, 255 = None
                let type_name = fn_name.strip_suffix("_partialCmp").unwrap_or(fn_name);
                if let Some(external_path) = type_to_external.get(type_name) {
                    format!(
                        "match (*(a as *const {local} as *const {ext})).partial_cmp(&*(b as *const {local} as *const {ext})) {{\n        \
                            Some(core::cmp::Ordering::Less) => 0,\n        \
                            Some(core::cmp::Ordering::Equal) => 1,\n        \
                            Some(core::cmp::Ordering::Greater) => 2,\n        \
                            None => 255,\n    \
                         }}",
                        ext = external_path,
                        local = type_name
                    )
                } else {
                    "match a.partial_cmp(b) {\n        \
                        Some(core::cmp::Ordering::Less) => 0,\n        \
                        Some(core::cmp::Ordering::Equal) => 1,\n        \
                        Some(core::cmp::Ordering::Greater) => 2,\n        \
                        None => 255,\n    \
                     }".to_string()
                }
            } else if fn_name.ends_with("_cmp") {
                // Extract type name from function name: AzTypeName_cmp -> AzTypeName
                // Returns: 0 = Less, 1 = Equal, 2 = Greater
                let type_name = fn_name.strip_suffix("_cmp").unwrap_or(fn_name);
                if let Some(external_path) = type_to_external.get(type_name) {
                    format!(
                        "match (*(a as *const {local} as *const {ext})).cmp(&*(b as *const {local} as *const {ext})) {{\n        \
                            core::cmp::Ordering::Less => 0,\n        \
                            core::cmp::Ordering::Equal => 1,\n        \
                            core::cmp::Ordering::Greater => 2,\n    \
                         }}",
                        ext = external_path,
                        local = type_name
                    )
                } else {
                    "match a.cmp(b) {\n        \
                        core::cmp::Ordering::Less => 0,\n        \
                        core::cmp::Ordering::Equal => 1,\n        \
                        core::cmp::Ordering::Greater => 2,\n    \
                     }".to_string()
                }
            } else if fn_name.ends_with("_hash") {
                // Extract type name from function name: AzTypeName_hash -> AzTypeName
                // Returns a u64 hash value
                let type_name = fn_name.strip_suffix("_hash").unwrap_or(fn_name);
                if let Some(external_path) = type_to_external.get(type_name) {
                    format!(
                        "{{\n        \
                            use core::hash::{{Hash, Hasher}};\n        \
                            let mut hasher = std::collections::hash_map::DefaultHasher::new();\n        \
                            (*(object as *const {local} as *const {ext})).hash(&mut hasher);\n        \
                            hasher.finish()\n    \
                         }}",
                        ext = external_path,
                        local = type_name
                    )
                } else {
                    "{\n        \
                        use core::hash::{Hash, Hasher};\n        \
                        let mut hasher = std::collections::hash_map::DefaultHasher::new();\n        \
                        object.hash(&mut hasher);\n        \
                        hasher.finish()\n    \
                     }".to_string()
                }
            } else {
                // Use fn_body from api.json - REQUIRED for all functions
                if let Some(body) = &fn_info.fn_body {
                    // Transform the fn_body to use transmute for type conversion
                    // The fn_body uses types without Az prefix, we need to add transmutes
                    // Now all arguments are transmuted, not just self
                    generate_transmuted_fn_body(
                        body,
                        &fn_info.class_name,
                        fn_info.is_constructor,
                        &fn_info.return_type,
                        prefix,
                        &type_to_external,
                        &fn_info.fn_args,
                        config.is_for_dll,
                        false, // C-API uses class_name for self variable, not "self"
                        false, // No force_clone_self for C-API
                        &std::collections::HashSet::new(), // No pre-converted args for C-API
                    )
                } else {
                    // No fn_body in api.json - ERROR! All functions must have fn_body defined
                    return Err(format!(
                        "ERROR: Function '{}' has no fn_body defined in api.json. All functions must \
                         have a fn_body to prevent unimplemented!() stubs in generated code.",
                        fn_name
                    ));
                }
            };

            let no_mangle_attr = if config.generate_no_mangle {
                "#[no_mangle]\n    "
            } else {
                ""
            };
            dll_code.push_str(&format!(
                "    #[allow(unused_variables)]\n    {}pub unsafe extern \"C\" fn \
                 {}({}){} {{ {} }}\n",
                no_mangle_attr, fn_name, fn_info.fn_args, return_str, fn_body
            ));
        }
        dll_code.push_str("    // --- End C-ABI Functions ---\n\n");
    }

    // NOTE: dll.rs patch is excluded for memtest - we only need struct definitions for memory
    // layout tests The patch contains impl blocks that reference missing functions/types

    dll_code.push_str("}\n\n");
    Ok(dll_code)
}

/// Generate public API modules with re-exports and patches
fn generate_public_api_modules(
    version_data: &VersionData,
    prefix: &str,
    config: &MemtestConfig,
    replacements: &TypeReplacements,
) -> Result<String> {
    let patch_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src/codegen/api-patch");

    // Map patch files to their module names
    // For memtest, we don't need any patches - only struct definitions for memory layout tests
    // All patches are excluded to avoid compilation errors from missing functions/macros
    let patches: Vec<(&str, &str)> = vec![
        // ("string.rs", "str"), // Excluded: contains impl blocks that reference missing types
        // ("vec.rs", "vec"), // Excluded: impl_vec! macros conflict with derives
        // ("option.rs", "option"), // Excluded: impl_option! macros conflict with derives
        // ("dom.rs", "dom"), // Excluded: contains macro calls and missing types
        // ("gl.rs", "gl"), // Excluded: VecRef methods auto-generated, GL types added to dll module
        // ("css.rs", "css"), // Excluded: contains macro calls and missing variants
        // ("window.rs", "window"), // Excluded: contains impl blocks that reference missing types
        // ("callbacks.rs", "callbacks"), // Excluded: callback types need workspace search
    ];

    // All modules just get re-exports, no patches
    let modules_without_patches = vec!["str", "vec", "option", "dom", "gl", "css", "window"];

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
                replacements,
            )?);
        }

        output.push_str("}\n\n");
    }

    // Generate modules without patches (just re-exports, traits come from derives)
    for module_name in modules_without_patches {
        output.push_str(&format!("pub mod {} {{\n", module_name));
        output.push_str("    use core::ffi::c_void;\n");
        output.push_str("    use super::dll::*;\n\n");
        output.push_str(&generate_reexports(version_data, prefix, module_name)?);
        output.push_str("}\n\n");
    }

    Ok(output)
}

/// Parse function arguments string into individual (name, type) pairs
///
/// Input: "dom: &mut AzDom, children: AzDomVec"
/// Output: [("dom", "&mut AzDom"), ("children", "AzDomVec")]
pub fn parse_fn_args(fn_args: &str) -> Vec<(String, String)> {
    if fn_args.trim().is_empty() {
        return Vec::new();
    }

    let mut result = Vec::new();
    let mut depth = 0;
    let mut current = String::new();

    // Handle nested generics like Option<Vec<T>>
    for ch in fn_args.chars() {
        match ch {
            '<' => {
                depth += 1;
                current.push(ch);
            }
            '>' => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 => {
                if !current.trim().is_empty() {
                    if let Some((name, ty)) = parse_single_arg(&current) {
                        result.push((name, ty));
                    }
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }

    // Don't forget the last argument
    if !current.trim().is_empty() {
        if let Some((name, ty)) = parse_single_arg(&current) {
            result.push((name, ty));
        }
    }

    result
}

/// Parse a single argument like "dom: &mut AzDom" into ("dom", "&mut AzDom")
fn parse_single_arg(arg: &str) -> Option<(String, String)> {
    let trimmed = arg.trim();
    let colon_pos = trimmed.find(':')?;
    let name = trimmed[..colon_pos].trim().to_string();
    let ty = trimmed[colon_pos + 1..].trim().to_string();
    Some((name, ty))
}

/// Generate a function body that transmutes between local (Az-prefixed) and external types
///
/// The fn_body in api.json uses unprefixed types like "Dom::new(node_type)"
/// We need to:
/// 1. Convert the self parameter from Az-prefixed local type to external type (transmute in)
/// 2. Convert ALL arguments from Az-prefixed local types to external types (transmute in)
/// 3. Call the actual function on the external type
/// 4. Convert the result back to Az-prefixed local type (transmute out)
///
/// Now generates multi-line readable code instead of one giant line.
pub fn generate_transmuted_fn_body(
    fn_body: &str,
    class_name: &str,
    is_constructor: bool,
    return_type: &str,
    prefix: &str,
    type_to_external: &std::collections::HashMap<String, String>,
    fn_args: &str,
    is_for_dll: bool,
    keep_self_name: bool, // If true, use "_self" for self parameter (for PyO3 bindings)
    force_clone_self: bool, // If true, always clone self (for PyO3 methods where API says self by-value)
    skip_args: &std::collections::HashSet<String>, // Arguments to skip (already converted with _ffi suffix)
) -> String {
    let self_var = class_name.to_lowercase();
    let parsed_args = parse_fn_args(fn_args);
    
    // For PyO3 bindings (keep_self_name=true), we need to use "_self" as the transmuted variable
    // because Rust doesn't allow shadowing "self"
    let transmuted_self_var = if keep_self_name { "_self" } else { &self_var };

    // Transform the fn_body:
    // 1. For DLL mode: Replace "azul_dll::" with "crate::" (generated code is included in azul-dll
    //    crate) For memtest mode: Keep "azul_dll::" as is (memtest uses azul_dll as dependency)
    // 2. Replace "self." and "classname." with the appropriate variable name
    // 3. Replace "object." with the appropriate variable name (legacy naming convention)
    // 4. Replace unqualified "TypeName::method(" with fully qualified path
    // 5. Replace standalone variable name (as function argument) with transmuted variable
    let mut fn_body = if is_for_dll {
        fn_body.replace("azul_dll::", "crate::")
    } else {
        fn_body.to_string()
    };

    // Only replace "self." with the transmuted variable name, but keep "classname." as-is
    // since we generate an alias `let classname = _self;` below
    fn_body = fn_body.replace("self.", &format!("{}.", transmuted_self_var));
    fn_body = fn_body.replace("object.", &format!("{}.", transmuted_self_var));

    // For constructors: if fn_body starts with "TypeName::" (no "::" before it),
    // replace with the fully qualified external path
    // E.g., "RefAny::new_c(...)" -> "azul_core::refany::RefAny::new_c(...)"
    if is_constructor {
        // Check if fn_body starts with a type name (uppercase letter followed by ::)
        if let Some(colon_pos) = fn_body.find("::") {
            let potential_type = &fn_body[..colon_pos];
            // Check if it's a simple type name (no :: in it, starts with uppercase)
            if !potential_type.contains("::")
                && potential_type
                    .chars()
                    .next()
                    .map(|c| c.is_uppercase())
                    .unwrap_or(false)
            {
                // Look up the type in type_to_external
                let prefixed_type = format!("{}{}", prefix, potential_type);
                if let Some(external_path) = type_to_external.get(&prefixed_type) {
                    // Replace "TypeName::" with "external::path::TypeName::"
                    let replacement = if is_for_dll {
                        format!("{}::", external_path.replace("azul_dll", "crate"))
                    } else {
                        format!("{}::", external_path)
                    };
                    fn_body = fn_body.replacen(&format!("{}::", potential_type), &replacement, 1);
                }
            }
        }
    }

    let mut lines = Vec::new();
    
    // Track if self is a reference - needed for PyO3 bindings where we need to clone
    // for consuming methods (builder pattern)
    let mut self_is_ref = false;

    // Generate transmutations for ALL arguments on separate lines
    for (arg_name, arg_type) in &parsed_args {
        // Skip arguments that are already converted (have _ffi suffix handled elsewhere)
        if skip_args.contains(arg_name) {
            continue;
        }
        
        let (is_ref, is_mut, base_type) = parse_arg_type(arg_type);
        
        // Track if self is a reference
        if arg_name == "self" {
            self_is_ref = is_ref || is_mut;
        }

        // Get the external type for this argument
        // For DLL mode: Replace azul_dll with crate since generated code is included in azul-dll
        // For memtest mode: Keep azul_dll as is since memtest uses azul_dll as dependency
        let external_type = if is_for_dll {
            type_to_external
                .get(&base_type)
                .map(|s| s.replace("azul_dll", "crate"))
                .unwrap_or_else(|| base_type.clone())
        } else {
            type_to_external
                .get(&base_type)
                .cloned()
                .unwrap_or_else(|| base_type.clone())
        };

        // For PyO3 bindings, use "_self" instead of "self" because Rust doesn't allow shadowing self
        let var_name = if keep_self_name && arg_name == "self" {
            "_self"
        } else {
            arg_name.as_str()
        };

        // Generate transmute line based on reference type
        let transmute_line = if is_mut {
            format!(
                "    let {var_name}: &mut {ext} = core::mem::transmute({arg_name});",
                var_name = var_name,
                arg_name = arg_name,
                ext = external_type
            )
        } else if is_ref {
            format!(
                "    let {var_name}: &{ext} = core::mem::transmute({arg_name});",
                var_name = var_name,
                arg_name = arg_name,
                ext = external_type
            )
        } else {
            format!(
                "    let {var_name}: {ext} = core::mem::transmute({arg_name});",
                var_name = var_name,
                arg_name = arg_name,
                ext = external_type
            )
        };

        lines.push(transmute_line);
    }

    // For PyO3 bindings (keep_self_name=true), generate an alias from the lowercase class name
    // to _self, so that fn_body can use the original variable name (e.g., `instant` for Instant)
    // This avoids having to replace all occurrences of the variable name in fn_body
    // IMPORTANT: If self is a reference AND fn_body uses consuming methods (builder pattern),
    // we need to clone. Builder methods like .with_*() consume self.
    // But for methods that just use references (like encode_bmp()), we should NOT clone.
    // ALSO: If force_clone_self is true, we always clone (for PyO3 methods where API says self by-value)
    if keep_self_name && !is_constructor {
        // Detect if fn_body uses builder pattern (consuming methods)
        // Builder pattern methods typically are: .with_*, .set_*, etc. that return Self
        let uses_builder_pattern = fn_body.contains(&format!("{}.with_", self_var))
            || fn_body.contains(&format!("{}.set_", self_var))
            || fn_body.contains(&format!("{}.add_", self_var))
            || fn_body.contains("object.with_")
            || fn_body.contains("_self.with_");
        
        if force_clone_self || (self_is_ref && uses_builder_pattern) {
            // Clone for consuming methods - the fn_body calls methods like .with_node_type()
            // that take self by value, OR the API expects self by value but PyO3 gives us &self
            // Since fn_body uses _self (after object. -> _self. replacement), clone to _self
            // Use a temporary to avoid "use of moved value" error
            lines.push(format!("    let __cloned = _self.clone();"));
            // Now fn_body replacements: replace "_self." with "__cloned." below
        } else {
            lines.push(format!("    let {} = _self;", self_var));
        }
    }
    
    // If we cloned, replace _self with __cloned in fn_body
    if keep_self_name && !is_constructor {
        let uses_builder_pattern = fn_body.contains(&format!("{}.with_", self_var))
            || fn_body.contains(&format!("{}.set_", self_var))
            || fn_body.contains(&format!("{}.add_", self_var))
            || fn_body.contains("object.with_")
            || fn_body.contains("_self.with_");
        if force_clone_self || (self_is_ref && uses_builder_pattern) {
            fn_body = fn_body.replace("_self.", "__cloned.");
            fn_body = fn_body.replace(&format!("{}.", self_var), "__cloned.");
        }
    }

    // Check if fn_body contains statements (has `;` before the last expression)
    let has_statements = fn_body.contains(';');

    if return_type.is_empty() {
        // Void return - just call the function (side effects only)
        if has_statements {
            lines.push(format!("    {}", fn_body));
        } else {
            lines.push(format!("    let _: () = {};", fn_body));
        }
    } else {
        // Has return type - need to transmute the result
        // For DLL mode: Replace azul_dll with crate since generated code is included in azul-dll
        // For memtest mode: Keep azul_dll as is since memtest uses azul_dll as dependency
        let return_external = if is_for_dll {
            type_to_external
                .get(return_type)
                .map(|s| s.as_str())
                .unwrap_or(return_type)
                .replace("azul_dll", "crate")
        } else {
            type_to_external
                .get(return_type)
                .cloned()
                .unwrap_or_else(|| return_type.to_string())
        };

        if has_statements {
            // fn_body has statements - wrap in block and transmute the final result
            lines.push(format!(
                "    let __result: {} = {{ {} }};",
                return_external, fn_body
            ));
        } else {
            // Simple expression - assign to __result
            lines.push(format!(
                "    let __result: {} = {};",
                return_external, fn_body
            ));
        }

        // Transmute result back to local type
        // The From/Into traits handle conversion between wrapper types
        lines.push(format!(
            "    core::mem::transmute::<{ext}, {local}>(__result)",
            ext = return_external,
            local = return_type
        ));
    }

    // Join with newlines and wrap in block
    format!("{{\n{}\n}}", lines.join("\n"))
}

/// Parse a type string to extract reference/mut info and base type
///
/// "&mut AzDom" -> (true, true, "AzDom")
/// "&AzDom" -> (true, false, "AzDom")
/// "AzDom" -> (false, false, "AzDom")
pub fn parse_arg_type(ty: &str) -> (bool, bool, String) {
    let trimmed = ty.trim();

    if trimmed.starts_with("&mut ") {
        (true, true, trimmed[5..].trim().to_string())
    } else if trimmed.starts_with("&") {
        (true, false, trimmed[1..].trim().to_string())
    } else {
        (false, false, trimmed.to_string())
    }
}

/// Primitive types that should never get an Az prefix
const PRIMITIVE_TYPES: &[&str] = &[
    "bool", "f32", "f64", "fn", "i128", "i16", "i32", "i64", "i8", "isize", "slice", "u128", "u16",
    "u32", "u64", "u8", "usize", "c_void", "str", "char", "c_char", "c_schar", "c_uchar",
];

/// Single-letter types are usually generic type parameters
fn is_generic_type_param(type_name: &str) -> bool {
    type_name.len() == 1
        && type_name
            .chars()
            .next()
            .map(|c| c.is_ascii_uppercase())
            .unwrap_or(false)
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
        for (class_name, class_data) in &module_data.classes {
            // Skip primitive types and generic type parameters - they don't need re-exports
            if PRIMITIVE_TYPES.contains(&class_name.as_str()) || is_generic_type_param(class_name) {
                continue;
            }
            // Skip types without struct_fields or enum_fields - they aren't generated
            // Also include callback_typedef types
            if class_data.struct_fields.is_none()
                && class_data.enum_fields.is_none()
                && class_data.callback_typedef.is_none()
            {
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
    replacements: &TypeReplacements,
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
        for type_name in &replacements.type_names {
            let prefixed = format!("{}{}", prefix, type_name);
            adjusted_line = replacements.replace_type_in_line(&adjusted_line, type_name, &prefixed);
        }

        // SECOND: Replace all remaining Az-prefixed types with the versioned prefix
        // This prevents double-prefixing: Az -> Az1 (not Az -> Az1 -> Az11)
        adjusted_line = replacements.replace_az_types(&adjusted_line, prefix);

        // Transform paths from crate:: (final azul crate) to super:: (memtest module context)
        // In generated.rs we have: mod dll { }, pub mod vec { }, etc.
        // So crate::dll:: needs to become super::dll:: when inside pub mod vec
        adjusted_line = adjusted_line.replace("crate::dll::", "super::dll::");
        adjusted_line = adjusted_line.replace("crate::vec::", "super::vec::");
        // crate::str::String -> super::str::AzString (String in api.json becomes AzString with
        // prefix)
        adjusted_line = adjusted_line.replace(
            "crate::str::String",
            &format!("super::str::{}String", prefix),
        );
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
