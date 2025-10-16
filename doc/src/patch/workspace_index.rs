//! In-memory workspace index for fast type lookups
//!
//! This module parses all Rust files in the workspace once and keeps them in memory,
//! avoiding repeated file I/O and parsing operations.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use indexmap::IndexMap;
use once_cell::sync::Lazy;
use regex::Regex;
use syn::{File, Item};

use crate::api::{EnumVariantData, FieldData};

// Pre-compiled regexes for type searching (initialized once at startup)
static MACRO_INVOCATION_REGEX: Lazy<Regex> = Lazy::new(|| {
    // Matches type names inside macro invocations like: impl_vec!(Type, TypeVec, TypeDestructor)
    // Captures everything between "!(" and the closing ")"
    Regex::new(r"!\s*\(((?:[^()]|\([^()]*\))*)\)").unwrap()
});

/// Represents a parsed type definition in the workspace
#[derive(Debug, Clone)]
pub struct ParsedTypeInfo {
    /// The full path to the type (e.g., "azul_core::id::NodeId")
    pub full_path: String,
    /// The simple name of the type (e.g., "NodeId")
    pub type_name: String,
    /// The file path where this type is defined
    pub file_path: PathBuf,
    /// The module path within the file (e.g., ["id"])
    pub module_path: Vec<String>,
    /// The kind of type
    pub kind: TypeKind,
    /// Source code of just this item
    pub source_code: String,
}

#[derive(Debug, Clone)]
pub enum TypeKind {
    Struct {
        fields: IndexMap<String, FieldInfo>,
        has_repr_c: bool,
        doc: Option<String>,
    },
    Enum {
        variants: IndexMap<String, VariantInfo>,
        has_repr_c: bool,
        doc: Option<String>,
    },
    TypeAlias {
        target: String,
        doc: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub struct FieldInfo {
    pub name: String,
    pub ty: String,
    pub doc: Option<String>,
}

#[derive(Debug, Clone)]
pub struct VariantInfo {
    pub name: String,
    pub ty: Option<String>,
    pub doc: Option<String>,
}

/// A parsed Rust file with all its items
#[derive(Debug, Clone)]
pub struct ParsedFile {
    pub path: PathBuf,
    pub syntax_tree: String, // We store the unparsed source to avoid lifetime issues
    pub types: Vec<String>,  // Type names defined in this file
}

/// In-memory index of the entire workspace
pub struct WorkspaceIndex {
    /// Maps type names to their full information
    /// Key: type name (e.g., "NodeId")
    /// Value: Vec of all types with that name (may be multiple in different modules)
    pub types: HashMap<String, Vec<ParsedTypeInfo>>,

    /// Maps crate directories to their crate names
    /// Key: crate directory path
    /// Value: crate name (e.g., "azul_core")
    pub crate_names: HashMap<PathBuf, String>,

    /// All parsed files for reference
    pub files: HashMap<PathBuf, ParsedFile>,

    /// Raw source content by file path (for string-based search)
    pub file_sources: HashMap<PathBuf, String>,
}

impl WorkspaceIndex {
    /// Build a complete index of the workspace
    pub fn build(project_root: &Path) -> Result<Self> {
        println!("🔍 Building workspace index...");

        let mut index = WorkspaceIndex {
            types: HashMap::new(),
            crate_names: HashMap::new(),
            files: HashMap::new(),
            file_sources: HashMap::new(),
        };

        // Step 1: Find all crates and their names
        println!("  📦 Discovering crates...");
        index.crate_names = build_crate_name_map(project_root)?;
        println!("    Found {} crates", index.crate_names.len());

        // Step 2: Find all Rust source files (including dll/src/widgets)
        println!("  📂 Finding Rust source files...");
        let rust_files = find_all_rust_files(project_root)?;
        println!("    Found {} .rs files", rust_files.len());

        // Step 3: Read all file sources first (for string-based search)
        println!("  📄 Reading file sources...");
        for file_path in &rust_files {
            if let Ok(content) = fs::read_to_string(file_path) {
                index.file_sources.insert(file_path.clone(), content);
            }
        }
        println!("    Read {} files into memory", index.file_sources.len());

        // Step 4: Parse all files in parallel
        println!("  📖 Parsing all files...");
        use rayon::prelude::*;

        let parsed_files: Vec<_> = rust_files
            .par_iter()
            .filter_map(|file_path| {
                match parse_rust_file(file_path, project_root, &index.crate_names) {
                    Ok(parsed) => Some(parsed),
                    Err(e) => {
                        // Silently skip unparseable files (likely syntax errors or non-module
                        // files)
                        if !file_path.to_string_lossy().contains("/target/") {
                            eprintln!("    ⚠️  Failed to parse {}: {}", file_path.display(), e);
                        }
                        None
                    }
                }
            })
            .collect();

        println!("    Successfully parsed {} files", parsed_files.len());

        // Step 5: Build index from parsed files
        println!("  🗂️  Building type index...");
        for parsed_file in parsed_files {
            // Extract all type definitions from this file
            for type_info in extract_types_from_file(&parsed_file)? {
                index
                    .types
                    .entry(type_info.type_name.clone())
                    .or_insert_with(Vec::new)
                    .push(type_info);
            }

            index.files.insert(parsed_file.path.clone(), parsed_file);
        }

        let total_types: usize = index.types.values().map(|v| v.len()).sum();
        println!(
            "    Indexed {} unique type names ({} total definitions)",
            index.types.len(),
            total_types
        );

        Ok(index)
    }

    /// Look up a type by name
    pub fn find_type(&self, type_name: &str) -> Option<&[ParsedTypeInfo]> {
        self.types.get(type_name).map(|v| v.as_slice())
    }

    /// Look up a type by full path (e.g., "azul_core::id::NodeId")
    pub fn find_type_by_path(&self, type_path: &str) -> Option<&ParsedTypeInfo> {
        let type_name = type_path.split("::").last()?;
        let candidates = self.find_type(type_name)?;

        candidates.iter().find(|info| info.full_path == type_path)
    }

    /// Find a type by string-based search in source files
    /// This finds types even if they're defined via macros
    /// Strategy:
    /// 1. First check macro invocations (between "!(" and ")")
    /// 2. Then check struct/enum/type declarations
    /// 3. Last resort: just presence in file (likely import)
    pub fn find_type_by_string_search(&self, type_name: &str) -> Option<ParsedTypeInfo> {
        let mut best_match: Option<(&PathBuf, i32)> = None;

        for (file_path, source) in &self.file_sources {
            if !source.contains(type_name) {
                continue;
            }

            let score = self.calculate_match_score(source, type_name);

            if let Some((_, current_score)) = best_match {
                if score > current_score {
                    best_match = Some((file_path, score));
                }
            } else {
                best_match = Some((file_path, score));
            }
        }

        if let Some((file_path, _)) = best_match {
            if let Ok(full_path) = self.deduce_full_path(file_path, type_name) {
                return Some(ParsedTypeInfo {
                    full_path: full_path.clone(),
                    type_name: type_name.to_string(),
                    file_path: file_path.clone(),
                    module_path: vec![],
                    kind: TypeKind::Struct {
                        fields: IndexMap::new(),
                        has_repr_c: self
                            .file_sources
                            .get(file_path)
                            .map(|s| s.contains("#[repr(C)]"))
                            .unwrap_or(false),
                        doc: None,
                    },
                    source_code: String::new(),
                });
            }
        }

        None
    }

    /// Calculate match score for a type in source code
    /// Higher score = more likely to be the actual definition
    fn calculate_match_score(&self, source: &str, type_name: &str) -> i32 {
        let mut score = 0;

        // HIGHEST PRIORITY: Type appears in macro invocation
        // Example: impl_vec!(Type, TypeVec, TypeDestructor)
        for cap in MACRO_INVOCATION_REGEX.captures_iter(source) {
            if let Some(macro_content) = cap.get(1) {
                let content = macro_content.as_str();
                // Check if type_name appears as a separate identifier in macro args
                // Split by common separators: comma, semicolon, whitespace
                let tokens: Vec<&str> = content
                    .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .collect();

                if tokens.contains(&type_name) {
                    score += 1000; // Very high priority
                }
            }
        }

        // HIGH PRIORITY: Public declarations
        if source.contains(&format!("pub struct {}", type_name)) {
            score += 100;
        }
        if source.contains(&format!("pub enum {}", type_name)) {
            score += 100;
        }
        if source.contains(&format!("pub type {}", type_name)) {
            score += 100;
        }

        // MEDIUM PRIORITY: Non-public declarations
        if source.contains(&format!("struct {}", type_name)) {
            score += 50;
        }
        if source.contains(&format!("enum {}", type_name)) {
            score += 50;
        }
        if source.contains(&format!("type {}", type_name)) {
            score += 50;
        }

        // LOW PRIORITY: Just mentioned (likely import)
        // Already checked by contains() in caller
        if score == 0 {
            score = 1; // Minimal score for just being present
        }

        score
    }

    /// Deduce the full type path from file path and type name
    /// Example: css/src/props/layout/position.rs + LayoutBottom
    ///       -> azul_css::props::layout::position::LayoutBottom
    pub fn deduce_full_path(&self, file_path: &Path, type_name: &str) -> Result<String> {
        // Find which crate this file belongs to
        let mut crate_name: Option<&String> = None;
        let mut relative_path: Option<PathBuf> = None;

        for (crate_dir, name) in &self.crate_names {
            if file_path.starts_with(crate_dir) {
                crate_name = Some(name);
                relative_path = file_path
                    .strip_prefix(crate_dir)
                    .ok()
                    .map(|p| p.to_path_buf());
                break;
            }
        }

        let crate_name =
            crate_name.ok_or_else(|| anyhow::anyhow!("No crate found for {:?}", file_path))?;
        let relative_path =
            relative_path.ok_or_else(|| anyhow::anyhow!("Could not get relative path"))?;

        // Build module path from file path
        // Remove "src/" prefix and file extension
        let mut module_parts = Vec::new();

        for component in relative_path.components() {
            let component_str = component.as_os_str().to_string_lossy();

            // Skip "src" directory
            if component_str == "src" {
                continue;
            }

            // Handle file name
            if component_str.ends_with(".rs") {
                let file_stem = component_str.trim_end_matches(".rs");
                // Skip lib.rs and mod.rs - they don't add to the module path
                if file_stem != "lib" && file_stem != "mod" {
                    module_parts.push(file_stem.to_string());
                }
            } else {
                // It's a directory
                module_parts.push(component_str.to_string());
            }
        }

        // Build full path: crate_name::module::path::TypeName
        let mut full_path = crate_name.clone();
        for part in module_parts {
            full_path.push_str("::");
            full_path.push_str(&part);
        }
        full_path.push_str("::");
        full_path.push_str(type_name);

        Ok(full_path)
    }

    /// Convert a ParsedTypeInfo to API format
    pub fn to_oracle_info(&self, type_info: &ParsedTypeInfo) -> crate::discover::OracleTypeInfo {
        match &type_info.kind {
            TypeKind::Struct {
                fields, has_repr_c, ..
            } => {
                let mut api_fields = IndexMap::new();
                for (name, field) in fields {
                    api_fields.insert(
                        name.clone(),
                        FieldData {
                            r#type: field.ty.clone(),
                            doc: field.doc.clone(),
                            derive: None,
                        },
                    );
                }
                crate::discover::OracleTypeInfo {
                    correct_path: Some(type_info.full_path.clone()),
                    fields: api_fields,
                    variants: IndexMap::new(),
                    has_repr_c: *has_repr_c,
                    is_enum: false,
                }
            }
            TypeKind::Enum {
                variants,
                has_repr_c,
                ..
            } => {
                let mut api_variants = IndexMap::new();
                for (name, variant) in variants {
                    api_variants.insert(
                        name.clone(),
                        EnumVariantData {
                            r#type: variant.ty.clone(),
                            doc: variant.doc.clone(),
                        },
                    );
                }
                crate::discover::OracleTypeInfo {
                    correct_path: Some(type_info.full_path.clone()),
                    fields: IndexMap::new(),
                    variants: api_variants,
                    has_repr_c: *has_repr_c,
                    is_enum: true,
                }
            }
            TypeKind::TypeAlias { .. } => crate::discover::OracleTypeInfo {
                correct_path: Some(type_info.full_path.clone()),
                fields: IndexMap::new(),
                variants: IndexMap::new(),
                has_repr_c: false,
                is_enum: false,
            },
        }
    }
}

/// Build a map of crate directories to their normalized crate names
fn build_crate_name_map(project_root: &Path) -> Result<HashMap<PathBuf, String>> {
    let mut map = HashMap::new();

    let cargo_tomls = find_files_with_name(project_root, "Cargo.toml")?;

    for toml_path in cargo_tomls {
        if let Ok(content) = fs::read_to_string(&toml_path) {
            if let Ok(toml_value) = content.parse::<toml::Value>() {
                if let Some(package) = toml_value.get("package") {
                    if let Some(name) = package.get("name") {
                        if let Some(name_str) = name.as_str() {
                            let crate_dir = toml_path.parent().unwrap().to_path_buf();
                            // Normalize: azul-core -> azul_core
                            let normalized = name_str.replace('-', "_");
                            map.insert(crate_dir, normalized);
                        }
                    }
                }
            }
        }
    }

    Ok(map)
}

/// Find all Rust files in the workspace
fn find_all_rust_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    visit_dirs_with_filter(root, &mut files, |path| {
        path.extension() == Some(std::ffi::OsStr::new("rs"))
            && !path.to_string_lossy().contains("/target/")
            && !path.to_string_lossy().contains("/.git/")
    })?;
    Ok(files)
}

/// Find all files with a specific name
fn find_files_with_name(root: &Path, filename: &str) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    visit_dirs_with_filter(root, &mut files, |path| {
        path.file_name() == Some(std::ffi::OsStr::new(filename))
            && !path.to_string_lossy().contains("/target/")
    })?;
    Ok(files)
}

/// Visit all directories recursively
fn visit_dirs_with_filter<F>(dir: &Path, results: &mut Vec<PathBuf>, filter: F) -> Result<()>
where
    F: Fn(&Path) -> bool + Copy,
{
    if !dir.is_dir() {
        return Ok(());
    }

    // Skip certain directories
    if let Some(name) = dir.file_name() {
        let name_str = name.to_string_lossy();
        if name_str == "target" || name_str == ".git" || name_str.starts_with('.') {
            return Ok(());
        }
    }

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() && filter(&path) {
            results.push(path);
        } else if path.is_dir() {
            visit_dirs_with_filter(&path, results, filter)?;
        }
    }

    Ok(())
}

/// Parse a single Rust file and extract basic info
fn parse_rust_file(
    file_path: &Path,
    project_root: &Path,
    crate_names: &HashMap<PathBuf, String>,
) -> Result<ParsedFile> {
    let source = fs::read_to_string(file_path)
        .with_context(|| format!("Failed to read {}", file_path.display()))?;

    // Parse to verify it's valid Rust (this will catch syntax errors)
    let _syntax_tree: File = syn::parse_str(&source)
        .with_context(|| format!("Failed to parse {}", file_path.display()))?;

    // Quick scan for type names (we'll parse properly later)
    let type_names = extract_type_names_quick(&source);

    Ok(ParsedFile {
        path: file_path.to_path_buf(),
        syntax_tree: source,
        types: type_names,
    })
}

/// Quick extraction of type names without full parsing
fn extract_type_names_quick(source: &str) -> Vec<String> {
    let mut names = Vec::new();

    for line in source.lines() {
        let trimmed = line.trim();
        if let Some(name) = extract_type_from_line(trimmed) {
            names.push(name);
        }
    }

    names
}

fn extract_type_from_line(line: &str) -> Option<String> {
    // Match: pub struct TypeName, pub enum TypeName, pub type TypeName
    if line.starts_with("pub struct ") {
        return line
            .strip_prefix("pub struct ")
            .and_then(|s| s.split_whitespace().next())
            .map(|s| s.trim_matches('<').to_string());
    }
    if line.starts_with("pub enum ") {
        return line
            .strip_prefix("pub enum ")
            .and_then(|s| s.split_whitespace().next())
            .map(|s| s.trim_matches('<').to_string());
    }
    if line.starts_with("pub type ") {
        return line
            .strip_prefix("pub type ")
            .and_then(|s| s.split_whitespace().next())
            .map(|s| s.trim_end_matches('=').trim().to_string());
    }
    None
}

/// Extract all type definitions from a parsed file
fn extract_types_from_file(parsed_file: &ParsedFile) -> Result<Vec<ParsedTypeInfo>> {
    use quote::ToTokens;

    let syntax_tree: File = syn::parse_str(&parsed_file.syntax_tree)?;
    let mut types = Vec::new();

    // Determine the crate name and module path for this file
    // This is simplified - in reality you'd need more sophisticated module path detection
    let (crate_name, module_path) = infer_module_path(&parsed_file.path)?;

    for item in syntax_tree.items {
        match item {
            Item::Struct(s) => {
                let type_name = s.ident.to_string();
                let full_path = build_full_path(&crate_name, &module_path, &type_name);

                let mut fields = IndexMap::new();
                for field in s.fields.iter() {
                    if let Some(field_name) = field.ident.as_ref() {
                        let field_ty = field.ty.to_token_stream().to_string();
                        fields.insert(
                            field_name.to_string(),
                            FieldInfo {
                                name: field_name.to_string(),
                                ty: crate::autofix::clean_type_string(&field_ty),
                                doc: crate::autofix::extract_doc_comments(&field.attrs),
                            },
                        );
                    }
                }

                let has_repr_c = s.attrs.iter().any(|attr| {
                    attr.path().is_ident("repr")
                        && attr.meta.to_token_stream().to_string().contains("C")
                });

                types.push(ParsedTypeInfo {
                    full_path,
                    type_name,
                    file_path: parsed_file.path.clone(),
                    module_path: module_path.clone(),
                    kind: TypeKind::Struct {
                        fields,
                        has_repr_c,
                        doc: crate::autofix::extract_doc_comments(&s.attrs),
                    },
                    source_code: s.to_token_stream().to_string(),
                });
            }
            Item::Enum(e) => {
                let type_name = e.ident.to_string();
                let full_path = build_full_path(&crate_name, &module_path, &type_name);

                let mut variants = IndexMap::new();
                for variant in &e.variants {
                    let variant_name = variant.ident.to_string();
                    let variant_ty = if variant.fields.is_empty() {
                        None
                    } else {
                        let fields_str = variant
                            .fields
                            .iter()
                            .map(|f| f.ty.to_token_stream().to_string())
                            .collect::<Vec<_>>()
                            .join(", ");
                        Some(crate::autofix::clean_type_string(&fields_str))
                    };

                    variants.insert(
                        variant_name.clone(),
                        VariantInfo {
                            name: variant_name,
                            ty: variant_ty,
                            doc: crate::autofix::extract_doc_comments(&variant.attrs),
                        },
                    );
                }

                let has_repr_c = e.attrs.iter().any(|attr| {
                    attr.path().is_ident("repr")
                        && attr.meta.to_token_stream().to_string().contains("C")
                });

                types.push(ParsedTypeInfo {
                    full_path,
                    type_name,
                    file_path: parsed_file.path.clone(),
                    module_path: module_path.clone(),
                    kind: TypeKind::Enum {
                        variants,
                        has_repr_c,
                        doc: crate::autofix::extract_doc_comments(&e.attrs),
                    },
                    source_code: e.to_token_stream().to_string(),
                });
            }
            Item::Type(t) => {
                let type_name = t.ident.to_string();
                let full_path = build_full_path(&crate_name, &module_path, &type_name);
                let target = t.ty.to_token_stream().to_string();

                types.push(ParsedTypeInfo {
                    full_path,
                    type_name,
                    file_path: parsed_file.path.clone(),
                    module_path: module_path.clone(),
                    kind: TypeKind::TypeAlias {
                        target: crate::autofix::clean_type_string(&target),
                        doc: crate::autofix::extract_doc_comments(&t.attrs),
                    },
                    source_code: t.to_token_stream().to_string(),
                });
            }
            _ => {}
        }
    }

    Ok(types)
}

/// Infer the module path from a file path
/// Returns (crate_name, module_path)
fn infer_module_path(file_path: &Path) -> Result<(String, Vec<String>)> {
    // Find the crate root by looking for Cargo.toml
    let mut current = file_path.parent();
    let mut components = Vec::new();

    while let Some(dir) = current {
        if dir.join("Cargo.toml").exists() {
            // Found the crate root
            if let Ok(toml_content) = fs::read_to_string(dir.join("Cargo.toml")) {
                if let Ok(toml_value) = toml_content.parse::<toml::Value>() {
                    if let Some(package) = toml_value.get("package") {
                        if let Some(name) = package.get("name") {
                            if let Some(name_str) = name.as_str() {
                                let crate_name = name_str.replace('-', "_");

                                // Build module path from components
                                components.reverse();

                                // Remove "src" and the file name itself
                                if let Some(filename) = file_path.file_stem() {
                                    let filename_str = filename.to_string_lossy();
                                    if filename_str != "lib" && filename_str != "mod" {
                                        // File name becomes part of the module path
                                        // unless it's lib.rs or mod.rs
                                        let mut module_path = components;
                                        if filename_str != "main" {
                                            module_path.push(filename_str.to_string());
                                        }
                                        return Ok((crate_name, module_path));
                                    }
                                }

                                return Ok((crate_name, components));
                            }
                        }
                    }
                }
            }
            break;
        }

        // Add this directory component
        if let Some(dir_name) = dir.file_name() {
            let name = dir_name.to_string_lossy().to_string();
            if name != "src" {
                components.push(name);
            }
        }

        current = dir.parent();
    }

    // If we couldn't find a crate root, return a placeholder
    // This happens for files in examples or other non-library locations
    let filename = file_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");
    Ok(("unknown_crate".to_string(), vec![filename.to_string()]))
}

/// Build a full type path from components
fn build_full_path(crate_name: &str, module_path: &[String], type_name: &str) -> String {
    let mut parts = vec![crate_name.to_string()];
    parts.extend(module_path.iter().cloned());
    parts.push(type_name.to_string());
    parts.join("::")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deduce_full_path_simple() {
        // Create a minimal workspace index
        let mut crate_names = HashMap::new();
        crate_names.insert(PathBuf::from("/project/css"), "azul_css".to_string());

        let index = WorkspaceIndex {
            types: HashMap::new(),
            crate_names,
            files: HashMap::new(),
            file_sources: HashMap::new(),
        };

        // Test: css/src/props/layout/position.rs + LayoutBottom
        // Should be: azul_css::props::layout::position::LayoutBottom
        let file_path = PathBuf::from("/project/css/src/props/layout/position.rs");
        let result = index.deduce_full_path(&file_path, "LayoutBottom").unwrap();

        assert_eq!(result, "azul_css::props::layout::position::LayoutBottom");
    }

    #[test]
    fn test_deduce_full_path_lib_rs() {
        let mut crate_names = HashMap::new();
        crate_names.insert(PathBuf::from("/project/core"), "azul_core".to_string());

        let index = WorkspaceIndex {
            types: HashMap::new(),
            crate_names,
            files: HashMap::new(),
            file_sources: HashMap::new(),
        };

        // Test: core/src/lib.rs + NodeId
        // Should be: azul_core::NodeId (lib.rs doesn't add to path)
        let file_path = PathBuf::from("/project/core/src/lib.rs");
        let result = index.deduce_full_path(&file_path, "NodeId").unwrap();

        assert_eq!(result, "azul_core::NodeId");
    }

    #[test]
    fn test_deduce_full_path_mod_rs() {
        let mut crate_names = HashMap::new();
        crate_names.insert(PathBuf::from("/project/core"), "azul_core".to_string());

        let index = WorkspaceIndex {
            types: HashMap::new(),
            crate_names,
            files: HashMap::new(),
            file_sources: HashMap::new(),
        };

        // Test: core/src/callbacks/mod.rs + Callback
        // Should be: azul_core::callbacks::Callback (mod.rs doesn't add to path)
        let file_path = PathBuf::from("/project/core/src/callbacks/mod.rs");
        let result = index.deduce_full_path(&file_path, "Callback").unwrap();

        assert_eq!(result, "azul_core::callbacks::Callback");
    }

    #[test]
    fn test_deduce_full_path_nested_module() {
        let mut crate_names = HashMap::new();
        crate_names.insert(PathBuf::from("/project/dll"), "azul_dll".to_string());

        let index = WorkspaceIndex {
            types: HashMap::new(),
            crate_names,
            files: HashMap::new(),
            file_sources: HashMap::new(),
        };

        // Test: dll/src/widgets/button.rs + ButtonOnClick
        // Should be: azul_dll::widgets::button::ButtonOnClick
        let file_path = PathBuf::from("/project/dll/src/widgets/button.rs");
        let result = index.deduce_full_path(&file_path, "ButtonOnClick").unwrap();

        assert_eq!(result, "azul_dll::widgets::button::ButtonOnClick");
    }

    #[test]
    fn test_deduce_full_path_with_hyphen() {
        let mut crate_names = HashMap::new();
        // Crate name with hyphen is normalized to underscore
        crate_names.insert(PathBuf::from("/project/azul-css"), "azul_css".to_string());

        let index = WorkspaceIndex {
            types: HashMap::new(),
            crate_names,
            files: HashMap::new(),
            file_sources: HashMap::new(),
        };

        let file_path = PathBuf::from("/project/azul-css/src/color.rs");
        let result = index.deduce_full_path(&file_path, "ColorU").unwrap();

        assert_eq!(result, "azul_css::color::ColorU");
    }
}
