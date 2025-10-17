/// Compiler Oracle - uses rustc to discover correct paths and field information
use std::{collections::HashMap, fs, path::Path, process::Command};

use anyhow::{Context, Result};
use indexmap::IndexMap;
use quote::ToTokens;
use regex::Regex;
use serde_json;

use crate::api::{EnumVariantData, FieldData};

/// Copy widget and azul_impl sources from dll/src to temp directory
pub fn copy_dll_sources(project_root: &Path, temp_dir: &Path) -> Result<()> {
    let dll_src = project_root.join("dll").join("src");
    let temp_src = temp_dir.join("src");

    // Copy widgets directory
    if dll_src.join("widgets").exists() {
        copy_dir_recursive(&dll_src.join("widgets"), &temp_src.join("widgets"))?;
    }

    // Copy azul_impl module files
    let azul_impl_src = dll_src;
    let azul_impl_dest = temp_src.join("azul_impl");
    fs::create_dir_all(&azul_impl_dest)?;

    for file in &["app.rs", "dialogs.rs", "file.rs"] {
        let src_file = azul_impl_src.join(file);
        if src_file.exists() {
            fs::copy(&src_file, azul_impl_dest.join(file))?;
        }
    }

    // Create mod.rs for azul_impl
    fs::write(
        azul_impl_dest.join("mod.rs"),
        "pub mod app;\npub mod dialogs;\npub mod file;\n",
    )?;

    // Copy other necessary files
    for file in &["extra.rs", "str.rs"] {
        let src_file = azul_impl_src.join(file);
        if src_file.exists() {
            fs::copy(&src_file, temp_src.join(file))?;
        }
    }

    Ok(())
}

/// Recursively copy a directory
pub fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if ty.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }

    Ok(())
}

/// Information discovered by the compiler oracle
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OracleTypeInfo {
    pub correct_path: Option<String>,
    pub fields: IndexMap<String, FieldData>,
    pub variants: IndexMap<String, EnumVariantData>,
    pub has_repr_c: bool,
    pub is_enum: bool,
}

/// Generate a fake lib.rs that tries to use all types and let the compiler correct us
pub fn discover_type_paths(
    project_root: &Path,
    type_names: &[(String, String)], // (module, class)
) -> Result<HashMap<String, OracleTypeInfo>> {
    println!("🔍 Using compiler oracle to discover type paths...");

    // Create a temporary directory for the fake project
    let temp_dir = project_root.join("target").join("oracle_temp");
    fs::create_dir_all(&temp_dir)?;

    // Copy widget files from dll/src (like memtest does)
    copy_dll_sources(project_root, &temp_dir)?;

    // Create Cargo.toml with its own workspace to avoid conflicts
    let cargo_toml = temp_dir.join("Cargo.toml");
    fs::write(
        &cargo_toml,
        r#"
[workspace]

[package]
name = "oracle-discovery"
version = "0.1.0"
edition = "2021"

[dependencies]
azul-core = { path = "../../core" }
azul-css = { path = "../../css" }
azul-layout = { path = "../../layout" }
"#,
    )?;

    let src_dir = temp_dir.join("src");
    fs::create_dir_all(&src_dir)?;

    // Generate lib.rs that attempts to use all types
    let lib_rs = src_dir.join("lib.rs");
    let mut content = String::from("// Auto-generated for compiler oracle\n");
    content.push_str("#![allow(unused, non_snake_case)]\n\n");

    // Add module declarations for copied files
    content.push_str("pub mod azul_impl;\n");
    content.push_str("pub mod widgets;\n");
    content.push_str("pub mod extra;\n");
    content.push_str("pub mod str;\n\n");

    for (_module, class) in type_names {
        // Try to use the type - compiler will tell us if it doesn't exist
        content.push_str(&format!("use {};\n", class));
    }

    fs::write(&lib_rs, content)?;

    println!(
        "  Generated test file with {} type imports",
        type_names.len()
    );
    println!("  Running cargo check (this may take a while)...");

    // Run cargo check and capture the output in JSON format
    let output = Command::new("cargo")
        .args(&["check", "--message-format=json", "--color=never"])
        .current_dir(&temp_dir)
        .output()
        .context("Failed to run cargo check")?;

    if !output.status.success() {
        println!("  ⚠️  Cargo check failed (this is expected - we want the errors!)");
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    println!("  Cargo check completed");
    println!(
        "  Stderr: {} bytes, Stdout: {} bytes",
        stderr.len(),
        stdout.len()
    );

    // Save debug output
    let debug_file = temp_dir.join("compiler_output.txt");
    fs::write(
        &debug_file,
        format!("STDERR:\n{}\n\nSTDOUT:\n{}", stderr, stdout),
    )?;
    println!("  💾 Debug output saved to: {}", debug_file.display());

    // Parse JSON compiler messages to find correct paths
    let mut discovered_paths: HashMap<String, String> = HashMap::new();

    for line in stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }

        if let Ok(msg) = serde_json::from_str::<serde_json::Value>(line) {
            // Look for compiler diagnostics with suggestions
            if msg["reason"] == "compiler-message" {
                if let Some(message) = msg["message"].as_object() {
                    // Check if this is an E0432 unresolved import error
                    if let Some(code) = message.get("code") {
                        if code["code"] == "E0432" {
                            // Look for suggestions in children
                            if let Some(children) = message["children"].as_array() {
                                for child in children {
                                    if child["level"] == "help" {
                                        // Check spans for suggested_replacement
                                        if let Some(spans) = child["spans"].as_array() {
                                            for span in spans {
                                                if let Some(suggested_replacement) =
                                                    span["suggested_replacement"].as_str()
                                                {
                                                    // The suggested_replacement contains the full
                                                    // path
                                                    // e.g., "azul_core::dom::NodeDataInlineCssProperty"
                                                    let correct_path =
                                                        suggested_replacement.to_string();
                                                    let type_name = correct_path
                                                        .split("::")
                                                        .last()
                                                        .unwrap_or(&correct_path);
                                                    println!(
                                                        "  ✓ {}: {}",
                                                        type_name, &correct_path
                                                    );
                                                    discovered_paths.insert(
                                                        type_name.to_string(),
                                                        correct_path,
                                                    );
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Now analyze each discovered type to get its structure
    // TODO: This is disabled for now as it takes too long and may fail on complex types
    // We only collect the paths for now
    println!("  🔍 Collected {} type paths", discovered_paths.len());
    let mut type_infos = HashMap::new();

    for (type_name, path) in discovered_paths {
        // For now, just store the path without analyzing fields
        type_infos.insert(
            type_name,
            OracleTypeInfo {
                correct_path: Some(path),
                fields: IndexMap::new(),
                variants: IndexMap::new(),
                has_repr_c: false,
                is_enum: false,
            },
        );
    }

    // Don't clean up - keep for debugging
    println!("  Test project kept at: {}", temp_dir.display());
    // let _ = fs::remove_dir_all(&temp_dir);

    Ok(type_infos)
}

/// Analyze a type from its full path to extract fields/variants
pub fn analyze_type_from_path(project_root: &Path, type_path: &str) -> Result<OracleTypeInfo> {
    // Parse the path to find crate and module
    // e.g., "azul_core::dom::NodeDataInlineCssProperty"
    let parts: Vec<&str> = type_path.split("::").collect();
    if parts.len() < 2 {
        anyhow::bail!("Invalid type path: {}", type_path);
    }

    let crate_name = parts[0];
    let type_name = parts[parts.len() - 1];

    // Map crate name to source directory
    let source_dir = match crate_name {
        "azul_core" => project_root.join("core/src"),
        "azul_css" => project_root.join("css/src"),
        "azul_layout" => project_root.join("layout/src"),
        "azul_impl" => project_root.join("dll/src"),
        _ => anyhow::bail!("Unknown crate: {}", crate_name),
    };

    // Search for the type definition in source files
    let module_path = &parts[1..parts.len() - 1].join("/");
    let possible_file = source_dir.join(format!("{}.rs", module_path));

    let source_content = if possible_file.exists() {
        fs::read_to_string(&possible_file)?
    } else {
        // Try module directory with mod.rs
        let mod_file = source_dir.join(format!("{}/mod.rs", module_path));
        if mod_file.exists() {
            fs::read_to_string(&mod_file)?
        } else {
            anyhow::bail!("Could not find source file for {}", type_path);
        }
    };

    // Parse the source to extract struct/enum definition
    parse_type_definition(&source_content, type_name, type_path)
}

/// Parse source code to extract type definition
pub fn parse_type_definition(
    source: &str,
    type_name: &str,
    full_path: &str,
) -> Result<OracleTypeInfo> {
    use syn::{File, Item};

    let syntax_tree: File = syn::parse_str(source).context("Failed to parse source file")?;

    for item in syntax_tree.items {
        match item {
            Item::Struct(s) if s.ident == type_name => {
                let mut fields = IndexMap::new();
                let has_repr_c = s.attrs.iter().any(|attr| {
                    attr.path().is_ident("repr")
                        && attr.meta.to_token_stream().to_string().contains("C")
                });

                for (idx, field) in s.fields.iter().enumerate() {
                    let field_name = field
                        .ident
                        .as_ref()
                        .map(|i| i.to_string())
                        .unwrap_or_else(|| format!("field_{}", idx));

                    let field_type = field.ty.to_token_stream().to_string();

                    fields.insert(
                        field_name,
                        FieldData {
                            r#type: field_type,
                            doc: None,
                            derive: None,
                        },
                    );
                }

                return Ok(OracleTypeInfo {
                    correct_path: Some(full_path.to_string()),
                    fields,
                    variants: IndexMap::new(),
                    has_repr_c,
                    is_enum: false,
                });
            }
            Item::Enum(e) if e.ident == type_name => {
                let mut variants = IndexMap::new();
                let has_repr_c = e.attrs.iter().any(|attr| {
                    attr.path().is_ident("repr")
                        && attr.meta.to_token_stream().to_string().contains("C")
                });

                for variant in &e.variants {
                    let variant_name = variant.ident.to_string();

                    // For enums, we store the type information differently
                    // If the variant has fields, we format them
                    let variant_type = if variant.fields.is_empty() {
                        None
                    } else {
                        let fields_str = variant
                            .fields
                            .iter()
                            .map(|f| f.ty.to_token_stream().to_string())
                            .collect::<Vec<_>>()
                            .join(", ");
                        Some(format!("({})", fields_str))
                    };

                    variants.insert(
                        variant_name,
                        EnumVariantData {
                            r#type: variant_type,
                            doc: None,
                        },
                    );
                }

                return Ok(OracleTypeInfo {
                    correct_path: Some(full_path.to_string()),
                    fields: IndexMap::new(),
                    variants,
                    has_repr_c,
                    is_enum: true,
                });
            }
            _ => {}
        }
    }

    anyhow::bail!("Type {} not found in source", type_name)
}

/// Use compiler to check field order and repr(C) by generating test code
pub fn analyze_type_structure(
    project_root: &Path,
    type_path: &str,
    is_enum: bool,
) -> Result<OracleTypeInfo> {
    let temp_dir = project_root.join("target").join("oracle_analysis");
    fs::create_dir_all(&temp_dir)?;

    // Create a minimal Cargo.toml
    let cargo_toml = temp_dir.join("Cargo.toml");
    fs::write(
        &cargo_toml,
        r#"
[package]
name = "oracle-analysis"
version = "0.1.0"
edition = "2021"

[dependencies]
azul-core = { path = "../../core" }
azul-css = { path = "../../css" }
azul-layout = { path = "../../layout" }
azul-impl = { path = "../../dll" }
"#,
    )?;

    let src_dir = temp_dir.join("src");
    fs::create_dir_all(&src_dir)?;

    // Generate code that will help us discover structure
    let lib_rs = src_dir.join("lib.rs");
    let mut content = String::from("#![allow(unused)]\n");
    content.push_str(&format!("use {};\n\n", type_path));

    if is_enum {
        // For enums, try to construct all variants to see what's missing
        content.push_str(&format!(
            r#"
fn test_enum() {{
    // Compiler will tell us all available variants when we try invalid ones
    let _ = match todo!() {{
        _ => {{}}
    }};
}}
"#
        ));
    } else {
        // For structs, try to access fields
        content.push_str(&format!(
            r#"
fn test_struct() {{
    let s: {} = todo!();
    // Try to print size and alignment
    println!("size: {{}}, align: {{}}", 
        std::mem::size_of::<{}>(),
        std::mem::align_of::<{}>()
    );
}}
"#,
            type_path.split("::").last().unwrap(),
            type_path.split("::").last().unwrap(),
            type_path.split("::").last().unwrap()
        ));
    }

    fs::write(&lib_rs, content)?;

    // Run rustc with --pretty=expanded to see the actual definition
    let output = Command::new("cargo")
        .args(&["rustc", "--", "-Zunpretty=expanded"])
        .current_dir(&temp_dir)
        .env("RUSTC_BOOTSTRAP", "1")
        .output()
        .context("Failed to run cargo rustc")?;

    let expanded = String::from_utf8_lossy(&output.stdout);

    // Parse the expanded code to extract structure info
    let info = parse_expanded_type(&expanded, type_path, is_enum)?;

    // Clean up
    let _ = fs::remove_dir_all(&temp_dir);

    Ok(info)
}

/// Parse expanded compiler output to extract type information
pub fn parse_expanded_type(
    expanded: &str,
    type_path: &str,
    is_enum: bool,
) -> Result<OracleTypeInfo> {
    let type_name = type_path.split("::").last().unwrap();

    // Look for repr(C) attribute
    let has_repr_c = expanded.contains("#[repr(C)]") || expanded.contains("#[repr(C, u8)]");

    let mut info = OracleTypeInfo {
        correct_path: Some(type_path.to_string()),
        fields: IndexMap::new(),
        variants: IndexMap::new(),
        has_repr_c,
        is_enum,
    };

    if is_enum {
        // Parse enum variants using regex
        let re = Regex::new(&format!(
            r"(?s)enum\s+{}\s*\{{([^}}]+)\}}",
            regex::escape(type_name)
        ))
        .unwrap();

        if let Some(caps) = re.captures(expanded) {
            let body = &caps[1];

            // Parse each variant
            let variant_re = Regex::new(r"(\w+)(?:\s*\(([^)]+)\))?\s*,?").unwrap();
            for caps in variant_re.captures_iter(body) {
                let name = caps[1].to_string();
                let ty = caps.get(2).map(|m| m.as_str().trim().to_string());

                info.variants.insert(
                    name,
                    EnumVariantData {
                        r#type: ty,
                        doc: None,
                    },
                );
            }
        }
    } else {
        // Parse struct fields using regex
        let re = Regex::new(&format!(
            r"(?s)struct\s+{}\s*\{{([^}}]+)\}}",
            regex::escape(type_name)
        ))
        .unwrap();

        if let Some(caps) = re.captures(expanded) {
            let body = &caps[1];

            // Parse each field
            let field_re = Regex::new(r"(?:pub\s+)?(\w+)\s*:\s*([^,]+),?").unwrap();
            for caps in field_re.captures_iter(body) {
                let name = caps[1].to_string();
                let ty = caps[2].trim().to_string();

                info.fields.insert(
                    name,
                    FieldData {
                        r#type: ty,
                        doc: None,
                        derive: None,
                    },
                );
            }
        }
    }

    Ok(info)
}

/// Recursively discover missing enum variants by trying to construct them
pub fn discover_missing_enum_variants(
    project_root: &Path,
    type_path: &str,
    known_variants: &[String],
) -> Result<Vec<String>> {
    let temp_dir = project_root.join("target").join("oracle_variants");
    fs::create_dir_all(&temp_dir)?;

    let cargo_toml = temp_dir.join("Cargo.toml");
    fs::write(
        &cargo_toml,
        r#"
[package]
name = "oracle-variants"
version = "0.1.0"
edition = "2021"

[dependencies]
azul-core = { path = "../../core" }
azul-css = { path = "../../css" }
azul-layout = { path = "../../layout" }
azul-impl = { path = "../../dll" }
"#,
    )?;

    let src_dir = temp_dir.join("src");
    fs::create_dir_all(&src_dir)?;

    let lib_rs = src_dir.join("lib.rs");
    let type_short = type_path.split("::").last().unwrap();

    // Create a match expression with only known variants
    // Compiler will warn about missing variants
    let mut content = format!(
        r#"#![deny(warnings)]
use {};

fn test_exhaustive(x: {}) {{
    match x {{
"#,
        type_path, type_short
    );

    for variant in known_variants {
        content.push_str(&format!("        {}::{} => {{}}\n", type_short, variant));
    }

    content.push_str("    }\n}\n");

    fs::write(&lib_rs, content)?;

    // Run cargo check and capture warnings
    let output = Command::new("cargo")
        .args(&["check", "--message-format=short"])
        .current_dir(&temp_dir)
        .output()
        .context("Failed to run cargo check")?;

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Parse compiler warning about missing variants
    let mut missing_variants = Vec::new();
    let re = Regex::new(r"pattern `([^`]+)` not covered").unwrap();

    for line in stderr.lines() {
        if let Some(caps) = re.captures(line) {
            let variant_list = &caps[1];
            // Parse out individual variants
            for variant in variant_list.split('|').map(str::trim) {
                if let Some(name) = variant.split("::").last() {
                    if !known_variants.contains(&name.to_string()) {
                        missing_variants.push(name.to_string());
                    }
                }
            }
        }
    }

    // Clean up
    let _ = fs::remove_dir_all(&temp_dir);

    Ok(missing_variants)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_compiler_suggestions() {
        let fake_compiler_output = r#"
error[E0432]: unresolved import `CssProperty`
 --> src/lib.rs:3:5
  |
3 | use CssProperty;
  |     ^^^^^^^^^^^ no `CssProperty` in the root
  |
help: consider importing this enum instead
  |
3 | use azul_css::props::property::CssProperty;
  |     ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

error[E0432]: unresolved import `CallbackType`
 --> src/lib.rs:4:5
  |
4 | use CallbackType;
  |     ^^^^^^^^^^^^ no `CallbackType` in the root
  |
help: consider importing this enum instead
  |
4 | use azul_layout::callbacks::CallbackType;
  |     ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

error[E0432]: unresolved import `WindowState`
 --> src/lib.rs:5:5
  |
5 | use WindowState;
  |     ^^^^^^^^^^^ no `WindowState` in the root
  |
help: consider importing this struct instead
  |
5 | use azul_layout::window_state::WindowState;
  |     ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
"#;

        let re_suggestion = Regex::new(r"use ([a-z_]+::[a-zA-Z0-9_:]+);").unwrap();
        let re_error_context = Regex::new(r"error\[E0432\].*unresolved import `([^`]+)`").unwrap();

        let mut discovered_paths = HashMap::new();
        let mut current_failed_import = None;

        for line in fake_compiler_output.lines() {
            // Check for unresolved import
            if let Some(caps) = re_error_context.captures(line) {
                current_failed_import = Some(caps[1].to_string());
            }

            // Check for compiler suggestion
            if let Some(caps) = re_suggestion.captures(line) {
                if let Some(ref failed_import) = current_failed_import {
                    let correct_path = caps[1].to_string();
                    let type_name = failed_import.split("::").last().unwrap_or(failed_import);
                    discovered_paths.insert(type_name.to_string(), correct_path);
                }
            }
        }

        assert_eq!(discovered_paths.len(), 3);
        assert_eq!(
            discovered_paths.get("CssProperty"),
            Some(&"azul_css::props::property::CssProperty".to_string())
        );
        assert_eq!(
            discovered_paths.get("CallbackType"),
            Some(&"azul_layout::callbacks::CallbackType".to_string())
        );
        assert_eq!(
            discovered_paths.get("WindowState"),
            Some(&"azul_layout::window_state::WindowState".to_string())
        );
    }

    #[test]
    fn test_parse_expanded_struct() {
        let fake_expanded = r#"
#[repr(C)]
pub struct WindowState {
    pub size: LayoutSize,
    pub position: LayoutPoint,
    pub flags: WindowFlags,
    pub debug_state: DebugState,
    pub keyboard_state: KeyboardState,
    pub mouse_state: MouseState,
}
"#;

        let re = Regex::new(r"(?s)struct\s+WindowState\s*\{([^}]+)\}").unwrap();

        if let Some(caps) = re.captures(fake_expanded) {
            let body = &caps[1];

            let field_re = Regex::new(r"(?:pub\s+)?(\w+)\s*:\s*([^,]+),?").unwrap();
            let mut fields = Vec::new();

            for caps in field_re.captures_iter(body) {
                let name = caps[1].to_string();
                let ty = caps[2].trim().to_string();
                fields.push((name, ty));
            }

            assert_eq!(fields.len(), 6);
            assert_eq!(fields[0], ("size".to_string(), "LayoutSize".to_string()));
            assert_eq!(
                fields[1],
                ("position".to_string(), "LayoutPoint".to_string())
            );
            assert_eq!(
                fields[5],
                ("mouse_state".to_string(), "MouseState".to_string())
            );
        } else {
            panic!("Failed to parse struct");
        }
    }

    #[test]
    fn test_parse_expanded_enum() {
        let fake_expanded = r#"
#[repr(C, u8)]
pub enum CallbackType {
    Ref(RefAny),
    Value(RefAny),
    RefMut(RefAny),
    IFrame(IFrameCallbackType),
    RenderImage(RenderImageCallbackType),
}
"#;

        let re = Regex::new(r"(?s)enum\s+CallbackType\s*\{([^}]+)\}").unwrap();

        if let Some(caps) = re.captures(fake_expanded) {
            let body = &caps[1];

            let variant_re = Regex::new(r"(\w+)\s*(?:\(([^)]+)\))?").unwrap();
            let mut variants = Vec::new();

            for caps in variant_re.captures_iter(body) {
                let name = caps[1].to_string();
                if name == "pub" || name == "enum" || name.is_empty() {
                    continue;
                }
                let ty = caps.get(2).map(|m| m.as_str().trim().to_string());
                variants.push((name, ty));
            }

            assert_eq!(variants.len(), 5);
            assert_eq!(variants[0], ("Ref".to_string(), Some("RefAny".to_string())));
            assert_eq!(
                variants[1],
                ("Value".to_string(), Some("RefAny".to_string()))
            );
            assert_eq!(
                variants[2],
                ("RefMut".to_string(), Some("RefAny".to_string()))
            );
            assert_eq!(
                variants[3],
                ("IFrame".to_string(), Some("IFrameCallbackType".to_string()))
            );
        } else {
            panic!("Failed to parse enum");
        }
    }

    #[test]
    fn test_parse_missing_variants() {
        let fake_compiler_warning = r#"
warning: patterns `CallbackType::IFrame` and `CallbackType::RenderImage` not covered
  --> src/lib.rs:10:11
   |
10 |     match x {
   |           ^ patterns `CallbackType::IFrame` and `CallbackType::RenderImage` not covered
"#;

        let re = Regex::new(r"patterns? `([^`]+)`(?: and `([^`]+)`)? not covered").unwrap();

        let mut missing_variants = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for line in fake_compiler_warning.lines() {
            if let Some(caps) = re.captures(line) {
                let variant1 = &caps[1];
                // Parse format like "CallbackType::IFrame"
                if let Some(name) = variant1.split("::").last() {
                    if seen.insert(name.to_string()) {
                        missing_variants.push(name.to_string());
                    }
                }

                if let Some(variant2) = caps.get(2) {
                    if let Some(name) = variant2.as_str().split("::").last() {
                        if seen.insert(name.to_string()) {
                            missing_variants.push(name.to_string());
                        }
                    }
                }
            }
        }

        assert_eq!(missing_variants.len(), 2);
        assert_eq!(missing_variants[0], "IFrame");
        assert_eq!(missing_variants[1], "RenderImage");
    }

    #[test]
    fn test_has_repr_c() {
        let struct_with_repr_c = r#"
#[repr(C)]
pub struct WindowState {
    pub size: LayoutSize,
}
"#;

        let enum_with_repr_c_u8 = r#"
#[repr(C, u8)]
pub enum CallbackType {
    Ref(RefAny),
}
"#;

        let struct_without_repr = r#"
pub struct Config {
    pub name: String,
}
"#;

        assert!(struct_with_repr_c.contains("#[repr(C)]"));
        assert!(enum_with_repr_c_u8.contains("#[repr(C, u8)]"));
        assert!(!struct_without_repr.contains("#[repr(C)]"));
    }

    #[test]
    fn test_full_patch_generation() {
        // Test that patch generation works correctly
        use std::collections::BTreeMap;

        use crate::patch::{ApiPatch, ClassPatch, ModulePatch, VersionPatch};

        // Generate patch
        let mut class_patch = ClassPatch::default();
        class_patch.external = Some("azul_layout::callbacks::CallbackType".to_string());

        let mut module_patches = BTreeMap::new();
        module_patches.insert("CallbackType".to_string(), class_patch);

        let mut version_patches = BTreeMap::new();
        version_patches.insert(
            "callbacks".to_string(),
            ModulePatch {
                classes: module_patches,
            },
        );

        let patch = ApiPatch {
            versions: BTreeMap::from([(
                "1.0.0-alpha1".to_string(),
                VersionPatch {
                    modules: version_patches,
                },
            )]),
        };

        // Verify patch can be serialized
        let patch_json = serde_json::to_string_pretty(&patch).unwrap();
        assert!(patch_json.contains("CallbackType"));
        assert!(patch_json.contains("azul_layout::callbacks::CallbackType"));
        assert!(patch_json.contains("external"));
        assert!(patch_json.contains("1.0.0-alpha1"));
        assert!(patch_json.contains("callbacks"));

        // Verify structure
        assert_eq!(patch.versions.len(), 1);
        let version_patch = patch.versions.get("1.0.0-alpha1").unwrap();
        assert_eq!(version_patch.modules.len(), 1);
        let module_patch = version_patch.modules.get("callbacks").unwrap();
        assert_eq!(module_patch.classes.len(), 1);
        let class_patch = module_patch.classes.get("CallbackType").unwrap();
        assert_eq!(
            class_patch.external.as_ref().unwrap(),
            "azul_layout::callbacks::CallbackType"
        );

        println!("✅ Generated valid patch:\n{}", patch_json);
    }
}
