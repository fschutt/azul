// Code generation for memory layout tests
// This module generates a complete test crate that validates memory layouts

use std::{collections::HashMap, collections::HashSet, fs, path::Path};

use indexmap::IndexMap;

use super::{
    func_gen::build_functions_map,
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
        let after_ok = after_pos >= remaining.len() || !is_word_char(remaining.as_bytes()[after_pos]);
        
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
                while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
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
                    let has_valid_crate = valid_crate_prefixes.iter()
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
            variant_map.iter().next().map(|(name, data)| {
                (name.clone(), data.r#type.is_some())
            })
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
    output.push_str("    // Verify discriminants match between generated and transmuted external\n");

    // For variants without data, compare generated vs transmuted external discriminants
    for (idx, (variant_name, has_data)) in variants.iter().enumerate() {
        if !*has_data {
            output.push_str(&format!(
                "    assert_eq!(gen_disc_{}, transmuted_disc_{}, \"Discriminant mismatch for variant {}: external type has different discriminant value\");\n",
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

fn generate_generated_rs(
    api_data: &ApiData,
    config: &MemtestConfig,
    replacements: &TypeReplacements,
) -> Result<String> {
    println!("    [WAIT] Starting generated.rs creation...");
    let mut output = String::new();

    output.push_str("// Auto-generated API definitions from api.json for memtest\n");
    output.push_str(
        "#![allow(dead_code, unused_imports, non_camel_case_types, non_snake_case, unused_unsafe, \
         clippy::all)]\n",
    );
    output.push_str("#![deny(improper_ctypes_definitions)]\n\n");
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
        private_pointers: false,
        no_derive: false,
        wrapper_postfix: String::new(),
        is_memtest: true, // Skip generating custom_impls trait implementations
    };
    dll_code.push_str(
        &generate_structs(version_data, &structs_map, &struct_config).map_err(|e| e.to_string())?,
    );

    // Generate Debug/PartialEq/PartialOrd implementations for VecDestructor types
    // These contain function pointers which can't derive these traits, so we compare by pointer address
    println!("      [IMPL] Generating VecDestructor trait implementations...");
    dll_code.push_str("\n    // ===== VecDestructor Trait Implementations =====\n");
    dll_code.push_str("    // Function pointers compared by address as usize\n\n");
    
    for prefixed_name in structs_map.keys() {
        // Keys are already prefixed like "AzU8VecDestructor"
        if prefixed_name.ends_with("VecDestructor") && !prefixed_name.ends_with("VecDestructorType") {
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

"#, name = prefixed_name));

            // PartialOrd implementation
            dll_code.push_str(&format!(
                r#"    impl PartialOrd for {name} {{
        fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {{
            Some(self.cmp(other))
        }}
    }}

"#, name = prefixed_name));

            // Eq implementation (since PartialEq is implemented)
            dll_code.push_str(&format!(
                r#"    impl Eq for {name} {{ }}

"#, name = prefixed_name));

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

"#, name = prefixed_name));

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

"#, name = prefixed_name));
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

"#, name = prefixed_name, elem = prefixed_element));
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

"#, name = prefixed_name, elem = prefixed_element));
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
            let slice_method = if unprefixed_name == "Refstr" { "as_str" } else { "as_slice" };
            
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

"#, name = prefixed_name, method = slice_method));

            // Clone implementation (creates a new reference to same data)
            dll_code.push_str(&format!(
                r#"    impl Clone for {name} {{
        fn clone(&self) -> Self {{
            Self {{ ptr: self.ptr, len: self.len }}
        }}
    }}

"#, name = prefixed_name));

            // Copy implementation (VecRef is just a fat pointer, so it's Copy)
            dll_code.push_str(&format!(
                r#"    impl Copy for {name} {{}}

"#, name = prefixed_name));

            // PartialEq implementation (compare slices)
            dll_code.push_str(&format!(
                r#"    impl PartialEq for {name} {{
        fn eq(&self, other: &Self) -> bool {{
            self.{method}() == other.{method}()
        }}
    }}

"#, name = prefixed_name, method = slice_method));

            // PartialOrd implementation (always available)
            dll_code.push_str(&format!(
                r#"    impl PartialOrd for {name} {{
        fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {{
            self.{method}().partial_cmp(other.{method}())
        }}
    }}

"#, name = prefixed_name, method = slice_method));

            // Eq, Ord and Hash only for types that support it (not f32/f64)
            if supports_ord_hash {
                // Eq implementation (f32/f64 don't implement Eq due to NaN)
                dll_code.push_str(&format!(
                    r#"    impl Eq for {name} {{}}

"#, name = prefixed_name));

                // Ord implementation
                dll_code.push_str(&format!(
                    r#"    impl Ord for {name} {{
        fn cmp(&self, other: &Self) -> core::cmp::Ordering {{
            self.{method}().cmp(other.{method}())
        }}
    }}

"#, name = prefixed_name, method = slice_method));

                // Hash implementation
                dll_code.push_str(&format!(
                    r#"    impl core::hash::Hash for {name} {{
        fn hash<H: core::hash::Hasher>(&self, state: &mut H) {{
            self.{method}().hash(state)
        }}
    }}

"#, name = prefixed_name, method = slice_method));
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
    let mut type_to_external: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for module_data in version_data.api.values() {
        for (class_name, class_data) in &module_data.classes {
            if let Some(external) = &class_data.external {
                let prefixed_name = format!("{}{}", prefix, class_name);
                type_to_external.insert(prefixed_name, external.clone());
            }
        }
    }

    println!("      [TARGET] Generating function stubs...");
    // Generate unimplemented!() stubs for all exported C functions
    // EXCEPT for _deepCopy and _delete which get real implementations that cast to external types
    let functions_map = build_functions_map(version_data, prefix).map_err(|e| e.to_string())?;
    println!("      [LINK] Found {} functions", functions_map.len());
    dll_code.push_str("\n    // --- C-ABI Function Stubs ---\n");
    for (fn_name, (fn_args, fn_return)) in &functions_map {
        let return_str = if fn_return.is_empty() {
            "".to_string()
        } else {
            format!(" -> {}", fn_return)
        };

        // Generate real implementations for _deepCopy and _delete functions
        // These need to cast to the external type, call the trait method, and cast back
        let fn_body = if fn_name.ends_with("_deepCopy") {
            // Extract type name from function name: AzTypeName_deepCopy -> AzTypeName
            let type_name = fn_name.strip_suffix("_deepCopy").unwrap_or(fn_name);
            if let Some(external_path) = type_to_external.get(type_name) {
                // Cast to external type, clone, cast back
                format!(
                    "core::mem::transmute::<{ext}, {local}>((*(object as *const {local} as *const {ext})).clone())",
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
        } else {
            // All other functions get unimplemented!() stubs
            format!("unimplemented!(\"{}\")", fn_name)
        };

        dll_code.push_str(&format!(
            "    #[allow(unused_variables)]\n    pub unsafe extern \"C\" fn {}({}){} {{ {} }}\n",
            fn_name, fn_args, return_str, fn_body
        ));
    }
    dll_code.push_str("    // --- End C-ABI Function Stubs ---\n\n");

    // NOTE: dll.rs patch is excluded for memtest - we only need struct definitions for memory layout tests
    // The patch contains impl blocks that reference missing functions/types

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
        for (class_name, class_data) in &module_data.classes {
            // Skip primitive types and generic type parameters - they don't need re-exports
            if PRIMITIVE_TYPES.contains(&class_name.as_str()) || is_generic_type_param(class_name) {
                continue;
            }
            // Skip types without struct_fields or enum_fields - they aren't generated
            // Also include callback_typedef types
            if class_data.struct_fields.is_none() && class_data.enum_fields.is_none() && class_data.callback_typedef.is_none() {
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
        // crate::str::String -> super::str::AzString (String in api.json becomes AzString with prefix)
        adjusted_line = adjusted_line.replace("crate::str::String", &format!("super::str::{}String", prefix));
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
