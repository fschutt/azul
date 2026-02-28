//! In-memory workspace index for fast type lookups
//!
//! This module parses all Rust files in the workspace once and keeps them in memory,
//! avoiding repeated file I/O and parsing operations.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use indexmap::IndexMap;
use quote::ToTokens;
use syn::{File, Item, UseTree};

use crate::api::{EnumVariantData, FieldData};

/// Information discovered about a type from source code parsing
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OracleTypeInfo {
    pub correct_path: Option<String>,
    pub fields: IndexMap<String, FieldData>,
    pub variants: IndexMap<String, EnumVariantData>,
    pub repr: Option<String>,
    pub is_enum: bool,
}

/// Check if a type name is imported via a `use` statement in the given source.
/// If so, this file is NOT the definition site for that type.
fn is_type_imported_in_use_statement(source: &str, type_name: &str) -> bool {
    let Ok(syntax_tree) = syn::parse_file(source) else {
        return false;
    };

    for item in &syntax_tree.items {
        if let Item::Use(use_item) = item {
            if use_tree_contains_ident(&use_item.tree, type_name) {
                return true;
            }
        }
    }
    false
}

/// Recursively check if a UseTree contains the given identifier
fn use_tree_contains_ident(tree: &UseTree, ident: &str) -> bool {
    match tree {
        UseTree::Path(path) => use_tree_contains_ident(&path.tree, ident),
        UseTree::Name(name) => name.ident == ident,
        UseTree::Rename(rename) => rename.ident == ident || rename.rename == ident,
        UseTree::Glob(_) => false, // Can't determine from glob
        UseTree::Group(group) => group
            .items
            .iter()
            .any(|t| use_tree_contains_ident(t, ident)),
    }
}

/// Find all macro invocations in source code and return (macro_name, args_content)
fn find_macro_invocations(source: &str) -> Vec<(String, String)> {
    let Ok(syntax_tree) = syn::parse_file(source) else {
        return Vec::new();
    };

    let mut results = Vec::new();

    for item in &syntax_tree.items {
        if let Item::Macro(macro_item) = item {
            // Get macro name (last segment of path)
            let macro_name = macro_item
                .mac
                .path
                .segments
                .last()
                .map(|s| s.ident.to_string())
                .unwrap_or_default();

            // Get the tokens inside the macro
            let args = macro_item.mac.tokens.to_string();

            results.push((macro_name, args));
        }
    }

    results
}

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
        repr: Option<String>,
        doc: Option<Vec<String>>,
        /// Generic type parameters (e.g., ["T"] for PhysicalPosition<T>)
        generic_params: Vec<String>,
        /// Traits implemented via `impl Trait for Type` (e.g., ["Clone", "Drop"])
        implemented_traits: Vec<String>,
        /// Traits from #[derive(...)] attribute
        derives: Vec<String>,
    },
    Enum {
        variants: IndexMap<String, VariantInfo>,
        repr: Option<String>,
        doc: Option<Vec<String>>,
        /// Generic type parameters (e.g., ["T"] for CssPropertyValue<T>)
        generic_params: Vec<String>,
        /// Traits implemented via `impl Trait for Type` (e.g., ["Clone", "Drop"])
        implemented_traits: Vec<String>,
        /// Traits from #[derive(...)] attribute
        derives: Vec<String>,
    },
    TypeAlias {
        target: String,
        /// Generic base type (e.g., "CssPropertyValue") if this is a generic instantiation
        generic_base: Option<String>,
        /// Generic arguments (e.g., ["LayoutZIndex"]) for instantiation
        generic_args: Vec<String>,
        doc: Option<Vec<String>>,
    },
    /// Callback function pointer type: `extern "C" fn(...) -> ReturnType`
    CallbackTypedef {
        /// Arguments to the callback function
        fn_args: Vec<CallbackArgInfo>,
        /// Return type of the callback (None = void)
        returns: Option<String>,
        doc: Option<Vec<String>>,
    },
}

/// Information about a callback function argument
#[derive(Debug, Clone)]
pub struct CallbackArgInfo {
    /// The type of the argument (e.g., "c_void", "RefAny")
    pub ty: String,
    /// How the argument is passed - uses full RefKind for pointer support
    pub ref_kind: crate::api::RefKind,
}

#[derive(Debug, Clone)]
pub struct FieldInfo {
    pub name: String,
    pub ty: String,
    /// Reference kind: constptr (*const T), mutptr (*mut T), or value (T)
    pub ref_kind: crate::api::RefKind,
    pub doc: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct VariantInfo {
    pub name: String,
    pub ty: Option<String>,
    pub doc: Option<Vec<String>>,
}

/// Extract derive traits from #[derive(...)] attributes
fn extract_derives(attrs: &[syn::Attribute]) -> Vec<String> {
    let mut derives = Vec::new();

    for attr in attrs {
        if !attr.path().is_ident("derive") {
            continue;
        }

        // Parse the derive attribute content
        let attr_str = attr.meta.to_token_stream().to_string();
        // Format: derive(Trait1, Trait2, ...)
        if let Some(start) = attr_str.find('(') {
            if let Some(end) = attr_str.rfind(')') {
                let content = &attr_str[start + 1..end];
                // Split by comma and clean up each trait name
                for trait_name in content.split(',') {
                    let trait_name = trait_name.trim();
                    // Remove any path prefix (e.g., "core::fmt::Debug" -> "Debug")
                    let simple_name = trait_name.rsplit("::").next().unwrap_or(trait_name).trim();
                    if !simple_name.is_empty() {
                        derives.push(simple_name.to_string());
                    }
                }
            }
        }
    }

    derives
}

/// Extract implemented traits from `impl Trait for Type` blocks in parsed syn items
/// Returns a map from type name to list of traits implemented for that type
fn extract_implemented_traits_from_items(items: &[syn::Item]) -> BTreeMap<String, Vec<String>> {
    let mut traits_map: BTreeMap<String, Vec<String>> = BTreeMap::new();

    // Standard library traits we care about
    let standard_traits = [
        "Clone",
        "Copy",
        "Debug",
        "Default",
        "Display",
        "PartialEq",
        "Eq",
        "PartialOrd",
        "Ord",
        "Hash",
        "Drop",
        "Send",
        "Sync",
        "Sized",
        "From",
        "Into",
        "TryFrom",
        "TryInto",
        "AsRef",
        "AsMut",
        "Deref",
        "DerefMut",
    ];

    for item in items {
        if let syn::Item::Impl(impl_block) = item {
            // We only care about trait implementations, not inherent impls
            if let Some((_, trait_path, _)) = &impl_block.trait_ {
                // Get the trait name (last segment of the path)
                let trait_name = trait_path
                    .segments
                    .last()
                    .map(|s| s.ident.to_string())
                    .unwrap_or_default();

                // Only track standard library traits
                if !standard_traits.contains(&trait_name.as_str()) {
                    continue;
                }

                // Get the type name this is implemented for
                if let syn::Type::Path(type_path) = &*impl_block.self_ty {
                    let type_name = type_path
                        .path
                        .segments
                        .last()
                        .map(|s| s.ident.to_string())
                        .unwrap_or_default();

                    if !type_name.is_empty() && !trait_name.is_empty() {
                        traits_map.entry(type_name).or_default().push(trait_name);
                    }
                }
            }
        }
    }

    traits_map
}

/// Extract derives from impl_option! macro calls
/// Handles formats like:
/// - impl_option!(Type, OptionType, [Debug, Clone, Copy])
/// - impl_option!(Type, OptionType, copy = false, [Debug, Clone])
/// - impl_option!(Type, OptionType, copy = false, clone = false, [Debug])
fn extract_derives_from_impl_option_macro(tokens_str: &str) -> Vec<String> {
    let mut derives = Vec::new();

    // Find the [...] part containing the derives
    if let Some(bracket_start) = tokens_str.find('[') {
        if let Some(bracket_end) = tokens_str.rfind(']') {
            let derives_str = &tokens_str[bracket_start + 1..bracket_end];
            // Split by comma and clean up each derive name
            for derive_name in derives_str.split(',') {
                let derive_name = derive_name.trim();
                if !derive_name.is_empty() {
                    derives.push(derive_name.to_string());
                }
            }
        }
    }

    // If no [...] found, return default derives for backwards compatibility
    if derives.is_empty() {
        derives = vec![
            "Debug".to_string(),
            "Clone".to_string(),
            "PartialEq".to_string(),
            "PartialOrd".to_string(),
            "Copy".to_string(),
        ];
    }

    derives
}

/// Information about types generated by impl_vec! and impl_option! macros
#[derive(Debug, Clone)]
pub struct MacroGeneratedType {
    pub type_name: String,
    pub element_type: String,
    pub destructor_name: Option<String>,
    pub derives: Vec<String>,
    pub is_vec: bool, // true for impl_vec!, false for impl_option!
}

/// Extract types generated by impl_vec! and impl_option! macros from parsed syn items
/// Also extracts trait information from impl_vec_*! macro calls
fn extract_macro_generated_types_from_items(items: &[syn::Item]) -> Vec<MacroGeneratedType> {
    let mut generated_types: BTreeMap<String, MacroGeneratedType> = BTreeMap::new();

    // Mapping from macro name to trait it implements
    let macro_to_trait: &[(&str, &str)] = &[
        ("impl_vec_debug", "Debug"),
        ("impl_vec_clone", "Clone"),
        ("impl_vec_partialeq", "PartialEq"),
        ("impl_vec_partialord", "PartialOrd"),
        ("impl_vec_eq", "Eq"),
        ("impl_vec_ord", "Ord"),
        ("impl_vec_hash", "Hash"),
    ];

    for item in items {
        if let syn::Item::Macro(m) = item {
            let macro_name = m
                .mac
                .path
                .segments
                .last()
                .map(|s| s.ident.to_string())
                .unwrap_or_default();

            // Parse macro tokens as comma-separated identifiers
            let tokens_str = m.mac.tokens.to_string();
            let args: Vec<&str> = tokens_str.split(',').map(|s| s.trim()).collect();

            if macro_name == "impl_vec" && args.len() >= 2 {
                // impl_vec!(ElementType, VecTypeName, DestructorName)
                let element_type = args[0].to_string();
                let vec_type_name = args[1].to_string();
                let destructor_name = args.get(2).map(|s| s.to_string());

                generated_types.insert(
                    vec_type_name.clone(),
                    MacroGeneratedType {
                        type_name: vec_type_name,
                        element_type,
                        destructor_name,
                        derives: Vec::new(),
                        is_vec: true,
                    },
                );
            } else if macro_name == "impl_option" && args.len() >= 2 {
                // impl_option!(ElementType, OptionTypeName, [Derives]) or
                // impl_option!(ElementType, OptionTypeName, copy = false, [Derives])
                let element_type = args[0].to_string();
                let option_type_name = args[1].to_string();

                // Extract derives from the [...] part in the macro call
                let derives = extract_derives_from_impl_option_macro(&tokens_str);

                generated_types.insert(
                    option_type_name.clone(),
                    MacroGeneratedType {
                        type_name: option_type_name,
                        element_type,
                        destructor_name: None,
                        derives,
                        is_vec: false,
                    },
                );
            } else {
                // Check for impl_vec_*! trait macros
                for (trait_macro, trait_name) in macro_to_trait {
                    if macro_name == *trait_macro && args.len() >= 2 {
                        // impl_vec_debug!(ElementType, VecTypeName) or
                        // impl_vec_clone!(ElementType, VecTypeName, DestructorName)
                        let vec_type_name = args[1].to_string();

                        // Add trait to existing type or create placeholder
                        generated_types
                            .entry(vec_type_name.clone())
                            .and_modify(|t| {
                                if !t.derives.contains(&trait_name.to_string()) {
                                    t.derives.push(trait_name.to_string());
                                }
                            })
                            .or_insert_with(|| {
                                // Create placeholder - will be merged with impl_vec! later
                                MacroGeneratedType {
                                    type_name: vec_type_name,
                                    element_type: args[0].to_string(),
                                    destructor_name: args.get(2).map(|s| s.to_string()),
                                    derives: vec![trait_name.to_string()],
                                    is_vec: true,
                                }
                            });
                        break;
                    }
                }
            }
        }
    }

    generated_types.into_values().collect()
}

/// Extract trait implementations from impl_vec_*! macro calls in parsed syn items
/// Returns a map from type name to list of traits implemented via macros
fn extract_impl_vec_traits_from_items(
    items: &[syn::Item],
) -> std::collections::BTreeMap<String, Vec<String>> {
    let mut traits_map: BTreeMap<String, Vec<String>> = BTreeMap::new();

    let macro_to_trait: &[(&str, &str)] = &[
        ("impl_vec_debug", "Debug"),
        ("impl_vec_clone", "Clone"),
        ("impl_vec_partialeq", "PartialEq"),
        ("impl_vec_partialord", "PartialOrd"),
        ("impl_vec_eq", "Eq"),
        ("impl_vec_ord", "Ord"),
        ("impl_vec_hash", "Hash"),
    ];

    for item in items {
        if let syn::Item::Macro(m) = item {
            let macro_name = m
                .mac
                .path
                .segments
                .last()
                .map(|s| s.ident.to_string())
                .unwrap_or_default();

            for (trait_macro, trait_name) in macro_to_trait {
                if macro_name == *trait_macro {
                    let tokens_str = m.mac.tokens.to_string();
                    let args: Vec<&str> = tokens_str.split(',').map(|s| s.trim()).collect();

                    if args.len() >= 2 {
                        let vec_type_name = args[1].to_string();
                        traits_map
                            .entry(vec_type_name)
                            .or_default()
                            .push(trait_name.to_string());
                    }
                    break;
                }
            }

            // Also handle impl_option! which provides several traits from the [...] part
            if macro_name == "impl_option" {
                let tokens_str = m.mac.tokens.to_string();
                let args: Vec<&str> = tokens_str.split(',').map(|s| s.trim()).collect();

                if args.len() >= 2 {
                    let option_type_name = args[1].to_string();
                    let derives = extract_derives_from_impl_option_macro(&tokens_str);
                    let traits = traits_map.entry(option_type_name).or_default();
                    for t in derives {
                        if !traits.contains(&t) {
                            traits.push(t);
                        }
                    }
                }
            }
        }
    }

    traits_map
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
    pub types: BTreeMap<String, Vec<ParsedTypeInfo>>,

    /// Maps crate directories to their crate names
    /// Key: crate directory path
    /// Value: crate name (e.g., "azul_core")
    pub crate_names: BTreeMap<PathBuf, String>,

    /// All parsed files for reference
    pub files: BTreeMap<PathBuf, ParsedFile>,

    /// Raw source content by file path (for string-based search)
    pub file_sources: BTreeMap<PathBuf, String>,
}

impl WorkspaceIndex {
    /// Build a complete index of the workspace
    pub fn build(project_root: &Path) -> Result<Self> {
        Self::build_with_verbosity(project_root, true)
    }

    /// Build a complete index of the workspace with optional quiet mode
    pub fn build_with_verbosity(project_root: &Path, verbose: bool) -> Result<Self> {
        if verbose {
            println!("[SEARCH] Building workspace index...");
        }

        let mut index = WorkspaceIndex {
            types: BTreeMap::new(),
            crate_names: BTreeMap::new(),
            files: BTreeMap::new(),
            file_sources: BTreeMap::new(),
        };

        // Step 1: Find all crates and their names
        if verbose {
            println!("  [PKG] Discovering crates...");
        }
        index.crate_names = build_crate_name_map(project_root)?;
        if verbose {
            println!("    Found {} crates", index.crate_names.len());
        }

        // Step 2: Find all Rust source files (including dll/src/widgets)
        if verbose {
            println!("  [DIR] Finding Rust source files...");
        }
        let rust_files = find_all_rust_files(project_root)?;
        if verbose {
            println!("    Found {} .rs files", rust_files.len());
        }

        // Step 3+4: Read and parse all files in parallel
        if verbose {
            println!("  📖 Reading and parsing all files in parallel...");
        }
        use rayon::prelude::*;

        let self_crate_name = env!("CARGO_PKG_NAME").replace('-', "_");
        let crate_names_ref = &index.crate_names;

        // Read all files in parallel first
        let file_contents: Vec<_> = rust_files
            .par_iter()
            .filter_map(|file_path| {
                // Skip files from the self crate (build tools, not part of API)
                let crate_name = crate_names_ref
                    .iter()
                    .find(|(crate_dir, _)| file_path.starts_with(crate_dir))
                    .map(|(_, name)| name.as_str());

                if let Some(name) = crate_name {
                    if name == self_crate_name {
                        return None;
                    }
                }

                // Read file content
                let content = fs::read_to_string(file_path).ok()?;
                Some((file_path.clone(), content))
            })
            .collect();

        if verbose {
            println!("    Read {} files into memory", file_contents.len());
        }

        // Now parse all files in parallel (using the already-read content)
        let parsed_files: Vec<_> = file_contents
            .par_iter()
            .filter_map(|(file_path, content)| {
                match parse_rust_file_from_content(
                    file_path,
                    content,
                    project_root,
                    crate_names_ref,
                ) {
                    Ok(parsed) => Some((file_path.clone(), content.clone(), parsed)),
                    Err(e) => {
                        // Silently skip unparseable files (likely syntax errors or non-module
                        // files)
                        if verbose && !file_path.to_string_lossy().contains("/target/") {
                            eprintln!("    [WARN]  Failed to parse {}: {}", file_path.display(), e);
                        }
                        None
                    }
                }
            })
            .collect();

        if verbose {
            println!("    Successfully parsed {} files", parsed_files.len());
        }

        // Step 5: Build index from parsed files and store file sources
        if verbose {
            println!("  🗂️  Building type index...");
        }
        for (file_path, content, parsed_file) in parsed_files {
            // Store file source for string-based search
            index.file_sources.insert(file_path.clone(), content);

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

        if verbose {
            let total_types: usize = index.types.values().map(|v| v.len()).sum();
            println!(
                "    Indexed {} unique type names ({} total definitions)",
                index.types.len(),
                total_types
            );
        }

        Ok(index)
    }

    /// Look up a type by name
    /// Will also try with "Az" prefix if not found (api.json uses "String", workspace has
    /// "AzString")
    pub fn find_type(&self, type_name: &str) -> Option<&[ParsedTypeInfo]> {
        // First try exact match
        if let Some(v) = self.types.get(type_name) {
            return Some(v.as_slice());
        }
        // Try with "Az" prefix (api.json has "String", workspace has "AzString")
        let az_prefixed = format!("Az{}", type_name);
        self.types.get(&az_prefixed).map(|v| v.as_slice())
    }

    /// Look up a type by full path (e.g., "azul_core::id::NodeId")
    pub fn find_type_by_path(&self, type_path: &str) -> Option<&ParsedTypeInfo> {
        let type_name = type_path.split("::").last()?;
        let candidates = self.find_type(type_name)?;

        // First try exact match
        if let Some(found) = candidates.iter().find(|info| info.full_path == type_path) {
            return Some(found);
        }

        // Try with "Az" prefix in path (api.json has "String", workspace has "AzString")
        let az_prefixed_path = {
            let parts: Vec<&str> = type_path.rsplitn(2, "::").collect();
            if parts.len() == 2 {
                format!("{}::Az{}", parts[1], parts[0])
            } else {
                format!("Az{}", type_path)
            }
        };
        candidates
            .iter()
            .find(|info| info.full_path == az_prefixed_path)
    }

    /// Find a type by string-based search in source files
    /// This finds types even if they're defined via macros
    /// Strategy:
    /// 1. First check macro invocations (between "!(" and ")")
    /// 2. Then check struct/enum/type declarations
    /// 3. Last resort: just presence in file (likely import)
    /// Also tries with "Az" prefix if not found (api.json uses "String", workspace has "AzString")
    pub fn find_type_by_string_search(&self, type_name: &str) -> Option<ParsedTypeInfo> {
        // Try exact match first
        if let Some(result) = self.find_type_by_string_search_inner(type_name) {
            return Some(result);
        }
        // Try with "Az" prefix (api.json has "String", workspace has "AzString")
        let az_prefixed = format!("Az{}", type_name);
        self.find_type_by_string_search_inner(&az_prefixed)
    }

    fn find_type_by_string_search_inner(&self, type_name: &str) -> Option<ParsedTypeInfo> {
        let mut best_match: Option<(&PathBuf, i32, bool)> = None; // (path, score, is_from_macro)

        // Self crate name to exclude build tool files
        let self_crate_name = env!("CARGO_PKG_NAME").replace('-', "_");

        for (file_path, source) in &self.file_sources {
            if !source.contains(type_name) {
                continue;
            }

            // Skip files from the self crate (build tools, not part of API)
            let crate_name = self
                .crate_names
                .iter()
                .find(|(crate_dir, _)| file_path.starts_with(crate_dir))
                .map(|(_, name)| name.as_str());

            if crate_name == Some(self_crate_name.as_str()) {
                continue;
            }

            // Skip dll/lib.rs - this is generated code with wrapper types, not definitions
            let path_str = file_path.to_string_lossy();
            if path_str.ends_with("dll/lib.rs") || path_str.ends_with("dll/python.rs") {
                continue;
            }

            let (score, is_from_macro) =
                self.calculate_match_score_with_macro_info(source, type_name);

            if let Some((_, current_score, _)) = best_match {
                if score > current_score {
                    best_match = Some((file_path, score, is_from_macro));
                }
            } else if score > 0 {
                best_match = Some((file_path, score, is_from_macro));
            }
        }

        if let Some((file_path, _score, is_from_macro)) = best_match {
            match self.deduce_full_path(file_path, type_name) {
                Ok(full_path) => {
                    // If type is from a macro like impl_vec! or define_spacing_property!,
                    // we know these macros generate #[repr(C)] types
                    let repr = if is_from_macro {
                        Some("C".to_string())
                    } else {
                        // Try to extract repr from source
                        self.file_sources.get(file_path).and_then(|s| {
                            // Simple extraction - look for #[repr(C)] or #[repr(C, u8)]
                            if s.contains("#[repr(C, u8)]") {
                                Some("C, u8".to_string())
                            } else if s.contains("#[repr(C)]") {
                                Some("C".to_string())
                            } else {
                                None
                            }
                        })
                    };

                    return Some(ParsedTypeInfo {
                        full_path: full_path.clone(),
                        type_name: type_name.to_string(),
                        file_path: file_path.clone(),
                        module_path: vec![],
                        kind: TypeKind::Struct {
                            fields: IndexMap::new(),
                            repr,
                            doc: None,
                            generic_params: Vec::new(),
                            implemented_traits: Vec::new(),
                            derives: Vec::new(),
                        },
                        source_code: String::new(),
                    });
                }
                Err(_e) => {
                    // Failed to deduce full path - skip this match
                }
            }
        }

        None
    }

    /// Calculate match score for a type in source code
    /// Higher score = more likely to be the actual definition
    /// Also returns whether the type was found in a macro invocation
    fn calculate_match_score_with_macro_info(&self, source: &str, type_name: &str) -> (i32, bool) {
        let mut score = 0;
        let mut is_from_macro = false;

        // Check if this type is IMPORTED via a use statement
        // If so, this file is NOT the definition site
        if is_type_imported_in_use_statement(source, type_name) {
            return (0, false);
        }

        // Macros that are known to generate #[repr(C)] types
        const REPR_C_MACROS: &[&str] = &[
            "impl_vec",
            "impl_vec_clone",
            "impl_option",
            "define_spacing_property",
        ];

        // HIGHEST PRIORITY: Type appears in macro invocation
        // Example: impl_vec!(Type, TypeVec, TypeDestructor)
        for (macro_name, macro_args) in find_macro_invocations(source) {
            // Check if type_name appears as a separate identifier in macro args
            let tokens: Vec<&str> = macro_args
                .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();

            if tokens.contains(&type_name) {
                score += 1000; // Very high priority

                // Check if this macro is known to generate #[repr(C)] types
                if REPR_C_MACROS.contains(&macro_name.as_str()) {
                    is_from_macro = true;
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
        if score == 0 {
            score = 1; // Minimal score for just being present
        }

        (score, is_from_macro)
    }

    /// Calculate match score for a type in source code
    /// Higher score = more likely to be the actual definition
    fn calculate_match_score(&self, source: &str, type_name: &str) -> i32 {
        let (score, _is_from_macro) = self.calculate_match_score_with_macro_info(source, type_name);
        score
    }

    /// Deduce the full type path from file path and type name
    /// Example: css/src/props/layout/position.rs + LayoutInsetBottom
    ///       -> azul_css::props::layout::position::LayoutInsetBottom
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

        // Fallback: Try to deduce crate name from path if not found in crate_names
        // This handles cases where file_sources contains files but crate_names lookup fails
        let (crate_name, relative_path) = if crate_name.is_none() {
            // Look for common crate directories in the path
            let path_str = file_path.to_string_lossy();
            let known_crates = [
                ("core/src/", "azul_core"),
                ("css/src/", "azul_css"),
                ("layout/src/", "azul_layout"),
                ("dll/src/", "azul_dll"),
                ("doc/src/", "azul_doc"),
            ];

            let mut found = None;
            for (pattern, crate_name) in known_crates {
                if let Some(idx) = path_str.find(pattern) {
                    let after_pattern = &path_str[idx + pattern.len()..];
                    found = Some((crate_name.to_string(), PathBuf::from(after_pattern)));
                    break;
                }
            }

            if let Some((name, rel_path)) = found {
                (name, rel_path)
            } else {
                return Err(anyhow::anyhow!("No crate found for {:?}", file_path));
            }
        } else {
            (crate_name.unwrap().clone(), relative_path.unwrap())
        };

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
    pub fn to_oracle_info(&self, type_info: &ParsedTypeInfo) -> OracleTypeInfo {
        match &type_info.kind {
            TypeKind::Struct {
                fields,
                repr,
                derives,
                ..
            } => {
                let mut api_fields = IndexMap::new();
                for (name, field) in fields {
                    api_fields.insert(
                        name.clone(),
                        FieldData {
                            r#type: field.ty.clone(),
                            ref_kind: field.ref_kind,
                            arraysize: None,
                            doc: field.doc.clone(),
                            derive: None,
                        },
                    );
                }
                OracleTypeInfo {
                    correct_path: Some(type_info.full_path.clone()),
                    fields: api_fields,
                    variants: IndexMap::new(),
                    repr: repr.clone(),
                    is_enum: false,
                }
            }
            TypeKind::Enum { variants, repr, .. } => {
                let mut api_variants = IndexMap::new();
                for (name, variant) in variants {
                    api_variants.insert(
                        name.clone(),
                        EnumVariantData {
                            r#type: variant.ty.clone(),
                            doc: variant.doc.clone(),
                            ref_kind: Default::default(),
                        },
                    );
                }
                OracleTypeInfo {
                    correct_path: Some(type_info.full_path.clone()),
                    fields: IndexMap::new(),
                    variants: api_variants,
                    repr: repr.clone(),
                    is_enum: true,
                }
            }
            TypeKind::TypeAlias { .. } => OracleTypeInfo {
                correct_path: Some(type_info.full_path.clone()),
                fields: IndexMap::new(),
                variants: IndexMap::new(),
                repr: None,
                is_enum: false,
            },
            TypeKind::CallbackTypedef { .. } => OracleTypeInfo {
                correct_path: Some(type_info.full_path.clone()),
                fields: IndexMap::new(),
                variants: IndexMap::new(),
                repr: None,
                is_enum: false,
            },
        }
    }
}

/// Build a map of crate directories to their normalized crate names
fn build_crate_name_map(project_root: &Path) -> Result<BTreeMap<PathBuf, String>> {
    let mut map = BTreeMap::new();

    let cargo_tomls = find_files_with_name(project_root, "Cargo.toml")?;

    for toml_path in &cargo_tomls {
        if let Ok(content) = fs::read_to_string(toml_path) {
            // Check if this is a [package] section (not just [workspace])
            if content.contains("[package]") {
                // Extract the package name using string parsing
                if let Some(name) = extract_cargo_package_name(&content) {
                    let crate_dir = toml_path.parent().unwrap().to_path_buf();
                    // Normalize: azul-core -> azul_core
                    let normalized = name.replace('-', "_");
                    map.insert(crate_dir, normalized);
                }
            }
        }
    }

    Ok(map)
}

/// Extract package name from Cargo.toml content
/// Looks for: name = "crate-name" or name = 'crate-name'
fn extract_cargo_package_name(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("name") {
            // Find the '=' sign
            if let Some(eq_pos) = trimmed.find('=') {
                let value_part = trimmed[eq_pos + 1..].trim();
                // Remove quotes (single or double)
                let name = value_part
                    .trim_start_matches('"')
                    .trim_start_matches('\'')
                    .trim_end_matches('"')
                    .trim_end_matches('\'');
                if !name.is_empty() {
                    return Some(name.to_string());
                }
            }
        }
    }
    None
}

/// Find all Rust files in the workspace
fn find_all_rust_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    visit_dirs_with_filter(root, &mut files, |path| {
        let path_str = path.to_string_lossy();
        path.extension() == Some(std::ffi::OsStr::new("rs"))
            && !path_str.contains("/target/")
            && !path_str.contains("/.git/")
            && !path_str.contains("/REFACTORING/") // Exclude refactoring files
            && !path_str.contains("/api/rust/src/") // Exclude generated API
    })?;
    Ok(files)
}

/// Find all files with a specific name
fn find_files_with_name(root: &Path, filename: &str) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    visit_dirs_with_filter(root, &mut files, |path| {
        let path_str = path.to_string_lossy();
        path.file_name() == Some(std::ffi::OsStr::new(filename))
            && !path_str.contains("/target/")
            && !path_str.contains("/REFACTORING/") // Exclude refactoring directory
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
        if name_str == "target" 
            || name_str == ".git" 
            || name_str == "REFACTORING" // Skip refactoring directory
            || name_str.starts_with('.')
        {
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
    crate_names: &BTreeMap<PathBuf, String>,
) -> Result<ParsedFile> {
    let source = fs::read_to_string(file_path)
        .with_context(|| format!("Failed to read {}", file_path.display()))?;

    parse_rust_file_from_content(file_path, &source, project_root, crate_names)
}

/// Parse a Rust file from already-read content (more efficient for parallel processing)
fn parse_rust_file_from_content(
    file_path: &Path,
    source: &str,
    _project_root: &Path,
    _crate_names: &BTreeMap<PathBuf, String>,
) -> Result<ParsedFile> {
    // Parse to verify it's valid Rust (this will catch syntax errors)
    let _syntax_tree: File = syn::parse_str(source)
        .with_context(|| format!("Failed to parse {}", file_path.display()))?;

    // Quick scan for type names (we'll parse properly later)
    let type_names = extract_type_names_quick(source);

    Ok(ParsedFile {
        path: file_path.to_path_buf(),
        syntax_tree: source.to_string(),
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

    // Extract implemented traits from impl blocks (impl Trait for Type)
    let impl_traits = extract_implemented_traits_from_items(&syntax_tree.items);

    for item in &syntax_tree.items {
        match item {
            Item::Struct(s) => {
                let type_name = s.ident.to_string();
                let full_path = build_full_path(&crate_name, &module_path, &type_name);

                // Extract generic type parameters (e.g., T from PhysicalPosition<T>)
                let generic_params: Vec<String> = s
                    .generics
                    .type_params()
                    .map(|tp| tp.ident.to_string())
                    .collect();

                let mut fields = IndexMap::new();
                for field in s.fields.iter() {
                    if let Some(field_name) = field.ident.as_ref() {
                        let field_ty = field.ty.to_token_stream().to_string();
                        // Extract base type and ref_kind from the type string
                        let (base_type, ref_kind) =
                            crate::autofix::utils::extract_type_and_ref_kind(&field_ty);
                        // Clean up the base type (path segments, etc.)
                        let cleaned_type = crate::autofix::utils::clean_type_string(&base_type);
                        fields.insert(
                            field_name.to_string(),
                            FieldInfo {
                                name: field_name.to_string(),
                                ty: cleaned_type,
                                ref_kind,
                                doc: crate::autofix::utils::extract_doc_comments(&field.attrs),
                            },
                        );
                    }
                }

                // Extract repr attribute value
                let repr = s.attrs.iter().find_map(|attr| {
                    if !attr.path().is_ident("repr") {
                        return None;
                    }
                    if let syn::Meta::List(list) = &attr.meta {
                        let tokens = list.tokens.to_string();
                        let repr_value = tokens
                            .split(',')
                            .map(|s| s.trim())
                            .collect::<Vec<_>>()
                            .join(", ");
                        if !repr_value.is_empty() {
                            return Some(repr_value);
                        }
                    }
                    None
                });

                // Extract derive traits from #[derive(...)] attributes
                let derives = extract_derives(&s.attrs);

                // Get implemented traits for this type (from impl Trait for Type blocks)
                let implemented_traits = impl_traits.get(&type_name).cloned().unwrap_or_default();

                types.push(ParsedTypeInfo {
                    full_path,
                    type_name: type_name.clone(),
                    file_path: parsed_file.path.clone(),
                    module_path: module_path.clone(),
                    kind: TypeKind::Struct {
                        fields,
                        repr,
                        doc: crate::autofix::utils::extract_doc_comments(&s.attrs),
                        generic_params,
                        implemented_traits,
                        derives,
                    },
                    source_code: s.to_token_stream().to_string(),
                });
            }
            Item::Enum(e) => {
                let type_name = e.ident.to_string();
                let full_path = build_full_path(&crate_name, &module_path, &type_name);

                // Extract generic type parameters (e.g., T from CssPropertyValue<T>)
                let generic_params: Vec<String> = e
                    .generics
                    .type_params()
                    .map(|tp| tp.ident.to_string())
                    .collect();

                let mut variants = IndexMap::new();
                for variant in &e.variants {
                    let variant_name = variant.ident.to_string();

                    // Check for repr(C) violations: variants can only have 0 or 1 field
                    let field_count = variant.fields.len();
                    if field_count > 1 {
                        eprintln!(
                            "[WARNING] repr(C) violation in enum '{}' variant '{}': has {} fields \
                             but repr(C) enums can only have 0 or 1 field per variant. File: {}",
                            type_name,
                            variant_name,
                            field_count,
                            parsed_file.path.display()
                        );
                        // Skip this variant - it cannot be represented in FFI
                        continue;
                    }

                    let variant_ty = if variant.fields.is_empty() {
                        None
                    } else {
                        // Exactly one field - safe for repr(C)
                        let field = variant.fields.iter().next().unwrap();
                        Some(crate::autofix::utils::clean_type_string(
                            &field.ty.to_token_stream().to_string(),
                        ))
                    };

                    variants.insert(
                        variant_name.clone(),
                        VariantInfo {
                            name: variant_name,
                            ty: variant_ty,
                            doc: crate::autofix::utils::extract_doc_comments(&variant.attrs),
                        },
                    );
                }

                // Extract repr attribute value
                let repr = e.attrs.iter().find_map(|attr| {
                    if !attr.path().is_ident("repr") {
                        return None;
                    }
                    if let syn::Meta::List(list) = &attr.meta {
                        let tokens = list.tokens.to_string();
                        let repr_value = tokens
                            .split(',')
                            .map(|s| s.trim())
                            .collect::<Vec<_>>()
                            .join(", ");
                        if !repr_value.is_empty() {
                            return Some(repr_value);
                        }
                    }
                    None
                });

                // Extract derive traits from #[derive(...)] attributes
                let derives = extract_derives(&e.attrs);

                // Get implemented traits for this type (from impl Trait for Type blocks)
                let implemented_traits = impl_traits.get(&type_name).cloned().unwrap_or_default();

                types.push(ParsedTypeInfo {
                    full_path,
                    type_name: type_name.clone(),
                    file_path: parsed_file.path.clone(),
                    module_path: module_path.clone(),
                    kind: TypeKind::Enum {
                        variants,
                        repr,
                        doc: crate::autofix::utils::extract_doc_comments(&e.attrs),
                        generic_params,
                        implemented_traits,
                        derives,
                    },
                    source_code: e.to_token_stream().to_string(),
                });
            }
            Item::Type(t) => {
                let type_name = t.ident.to_string();
                let full_path = build_full_path(&crate_name, &module_path, &type_name);
                let doc = crate::autofix::utils::extract_doc_comments(&t.attrs);
                let source_code = t.to_token_stream().to_string();

                // Check if this is an extern "C" fn type (callback typedef)
                if let syn::Type::BareFn(bare_fn) = &*t.ty {
                    // Parse the callback function arguments
                    let fn_args: Vec<CallbackArgInfo> = bare_fn
                        .inputs
                        .iter()
                        .map(|arg| parse_callback_arg(&arg.ty))
                        .collect();

                    // Parse the return type
                    let returns = match &bare_fn.output {
                        syn::ReturnType::Default => None,
                        syn::ReturnType::Type(_, ty) => {
                            let ret_str = ty.to_token_stream().to_string();
                            let cleaned = crate::autofix::utils::clean_type_string(&ret_str);
                            if cleaned.is_empty() || cleaned == "()" {
                                None
                            } else {
                                Some(cleaned)
                            }
                        }
                    };

                    types.push(ParsedTypeInfo {
                        full_path,
                        type_name,
                        file_path: parsed_file.path.clone(),
                        module_path: module_path.clone(),
                        kind: TypeKind::CallbackTypedef {
                            fn_args,
                            returns,
                            doc,
                        },
                        source_code,
                    });
                } else {
                    // Regular type alias
                    let (generic_base, generic_args) = parse_generic_type_alias(&t.ty);
                    let target = t.ty.to_token_stream().to_string();

                    types.push(ParsedTypeInfo {
                        full_path,
                        type_name,
                        file_path: parsed_file.path.clone(),
                        module_path: module_path.clone(),
                        kind: TypeKind::TypeAlias {
                            target: crate::autofix::utils::clean_type_string(&target),
                            generic_base,
                            generic_args,
                            doc,
                        },
                        source_code,
                    });
                }
            }
            _ => {}
        }
    }

    // Post-process: Extract traits from impl_vec_*! macro calls and add to Vec types
    // Re-parse the file to get the items for macro analysis
    let syntax_tree_for_macros: syn::File = syn::parse_str(&parsed_file.syntax_tree)
        .unwrap_or_else(|_| syn::File {
            shebang: None,
            attrs: vec![],
            items: vec![],
        });

    let impl_vec_traits = extract_impl_vec_traits_from_items(&syntax_tree_for_macros.items);
    for type_info in &mut types {
        if let Some(traits) = impl_vec_traits.get(&type_info.type_name) {
            match &mut type_info.kind {
                TypeKind::Struct { derives, .. } => {
                    // Add traits from impl_vec_*! macros to derives
                    for trait_name in traits {
                        if !derives.contains(trait_name) {
                            derives.push(trait_name.clone());
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // Also create synthetic types for impl_vec!/impl_option! generated types
    // These types are not directly visible to syn because they're generated by macros
    let macro_generated = extract_macro_generated_types_from_items(&syntax_tree_for_macros.items);
    let existing_type_names: std::collections::BTreeSet<String> =
        types.iter().map(|t| t.type_name.clone()).collect();

    for gen_type in macro_generated {
        // Only add if not already found by syn parser
        if !existing_type_names.contains(&gen_type.type_name) {
            let full_path = build_full_path(&crate_name, &module_path, &gen_type.type_name);

            // Create synthetic struct for Vec/Option types
            // Vec types have: ptr, len, cap, destructor
            // Option types have: None, Some variants
            if gen_type.is_vec {
                let mut fields = IndexMap::new();
                fields.insert(
                    "ptr".to_string(),
                    FieldInfo {
                        name: "ptr".to_string(),
                        ty: gen_type.element_type.clone(),
                        ref_kind: crate::api::RefKind::ConstPtr,
                        doc: None,
                    },
                );
                fields.insert(
                    "len".to_string(),
                    FieldInfo {
                        name: "len".to_string(),
                        ty: "usize".to_string(),
                        ref_kind: crate::api::RefKind::Value,
                        doc: None,
                    },
                );
                fields.insert(
                    "cap".to_string(),
                    FieldInfo {
                        name: "cap".to_string(),
                        ty: "usize".to_string(),
                        ref_kind: crate::api::RefKind::Value,
                        doc: None,
                    },
                );
                if let Some(destructor) = &gen_type.destructor_name {
                    fields.insert(
                        "destructor".to_string(),
                        FieldInfo {
                            name: "destructor".to_string(),
                            ty: destructor.clone(),
                            ref_kind: crate::api::RefKind::Value,
                            doc: None,
                        },
                    );
                }

                types.push(ParsedTypeInfo {
                    full_path,
                    type_name: gen_type.type_name.clone(),
                    file_path: parsed_file.path.clone(),
                    module_path: module_path.clone(),
                    kind: TypeKind::Struct {
                        fields,
                        repr: Some("C".to_string()),
                        doc: Some(vec![format!(
                            "Wrapper over a Rust-allocated `Vec<{}>",
                            gen_type.element_type
                        )]),
                        generic_params: Vec::new(),
                        implemented_traits: Vec::new(),
                        derives: gen_type.derives,
                    },
                    source_code: String::new(), // No source code for macro-generated types
                });

                // Also create the Destructor enum if it has a destructor
                if let Some(ref destructor_name) = gen_type.destructor_name {
                    if !existing_type_names.contains(destructor_name) {
                        let destructor_full_path =
                            build_full_path(&crate_name, &module_path, destructor_name);

                        // Create the destructor enum with variants: DefaultRust, NoDestructor,
                        // External
                        let mut variants = IndexMap::new();
                        variants.insert(
                            "DefaultRust".to_string(),
                            VariantInfo {
                                name: "DefaultRust".to_string(),
                                ty: None,
                                doc: None,
                            },
                        );
                        variants.insert(
                            "NoDestructor".to_string(),
                            VariantInfo {
                                name: "NoDestructor".to_string(),
                                ty: None,
                                doc: None,
                            },
                        );
                        // External variant takes a function pointer
                        variants.insert(
                            "External".to_string(),
                            VariantInfo {
                                name: "External".to_string(),
                                ty: Some(format!("extern \"C\" fn(*mut {})", gen_type.type_name)),
                                doc: None,
                            },
                        );

                        types.push(ParsedTypeInfo {
                            full_path: destructor_full_path,
                            type_name: destructor_name.clone(),
                            file_path: parsed_file.path.clone(),
                            module_path: module_path.clone(),
                            kind: TypeKind::Enum {
                                variants,
                                repr: Some("C".to_string()),
                                doc: Some(vec![format!("Destructor for `{}`", gen_type.type_name)]),
                                generic_params: Vec::new(),
                                implemented_traits: Vec::new(),
                                derives: vec![
                                    "Debug".to_string(),
                                    "Copy".to_string(),
                                    "Clone".to_string(),
                                ],
                            },
                            source_code: String::new(),
                        });
                    }
                }
            } else {
                // Option types - create synthetic enum with None/Some variants
                let mut variants = IndexMap::new();
                variants.insert(
                    "None".to_string(),
                    VariantInfo {
                        name: "None".to_string(),
                        ty: None,
                        doc: None,
                    },
                );
                variants.insert(
                    "Some".to_string(),
                    VariantInfo {
                        name: "Some".to_string(),
                        ty: Some(gen_type.element_type.clone()),
                        doc: None,
                    },
                );

                types.push(ParsedTypeInfo {
                    full_path,
                    type_name: gen_type.type_name.clone(),
                    file_path: parsed_file.path.clone(),
                    module_path: module_path.clone(),
                    kind: TypeKind::Enum {
                        variants,
                        repr: Some("C".to_string()),
                        doc: Some(vec![format!(
                            "Option<{}> but FFI-safe",
                            gen_type.element_type
                        )]),
                        generic_params: Vec::new(),
                        implemented_traits: Vec::new(),
                        derives: gen_type.derives,
                    },
                    source_code: String::new(),
                });
            }
        }
    }

    Ok(types)
}

/// Parse a type alias to extract generic base type and arguments
/// e.g., "CssPropertyValue<LayoutZIndex>" -> ("CssPropertyValue", ["LayoutZIndex"])
/// Parse a callback function argument to extract type and ref_kind
fn parse_callback_arg(ty: &syn::Type) -> CallbackArgInfo {
    use crate::api::RefKind;

    match ty {
        syn::Type::Ptr(ptr) => {
            // Handle *const T or *mut T
            let inner_type =
                crate::autofix::utils::clean_type_string(&ptr.elem.to_token_stream().to_string());
            let ref_kind = if ptr.mutability.is_some() {
                RefKind::MutPtr
            } else {
                RefKind::ConstPtr
            };
            CallbackArgInfo {
                ty: inner_type,
                ref_kind,
            }
        }
        syn::Type::Reference(reference) => {
            // Handle &T or &mut T
            let inner_type = crate::autofix::utils::clean_type_string(
                &reference.elem.to_token_stream().to_string(),
            );
            let ref_kind = if reference.mutability.is_some() {
                RefKind::RefMut
            } else {
                RefKind::Ref
            };
            CallbackArgInfo {
                ty: inner_type,
                ref_kind,
            }
        }
        _ => {
            // Value type
            let type_str =
                crate::autofix::utils::clean_type_string(&ty.to_token_stream().to_string());
            CallbackArgInfo {
                ty: type_str,
                ref_kind: RefKind::Value,
            }
        }
    }
}

fn parse_generic_type_alias(ty: &syn::Type) -> (Option<String>, Vec<String>) {
    match ty {
        syn::Type::Path(type_path) => {
            if let Some(segment) = type_path.path.segments.last() {
                let base_name = segment.ident.to_string();

                // Check if it has generic arguments
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    let generic_args: Vec<String> = args
                        .args
                        .iter()
                        .filter_map(|arg| {
                            match arg {
                                syn::GenericArgument::Type(syn::Type::Path(p)) => {
                                    // Extract the last segment of the path
                                    p.path.segments.last().map(|seg| seg.ident.to_string())
                                }
                                _ => None,
                            }
                        })
                        .collect();

                    if !generic_args.is_empty() {
                        return (Some(base_name), generic_args);
                    }
                }
            }
        }
        _ => {}
    }

    (None, vec![])
}

/// Infer the module path from a file path
/// Returns (crate_name, module_path)
fn infer_module_path(file_path: &Path) -> Result<(String, Vec<String>)> {
    // Find the crate root by looking for Cargo.toml
    let mut current = file_path.parent();
    let mut components = Vec::new();

    while let Some(dir) = current {
        let cargo_toml = dir.join("Cargo.toml");
        if cargo_toml.exists() {
            // Found the crate root
            if let Ok(toml_content) = fs::read_to_string(&cargo_toml) {
                if let Ok(toml_table) = toml_content.parse::<toml::Table>() {
                    if let Some(package) = toml_table.get("package") {
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
        let mut crate_names = BTreeMap::new();
        crate_names.insert(PathBuf::from("/project/css"), "azul_css".to_string());

        let index = WorkspaceIndex {
            types: BTreeMap::new(),
            crate_names,
            files: BTreeMap::new(),
            file_sources: BTreeMap::new(),
        };

        // Test: css/src/props/layout/position.rs + LayoutInsetBottom
        // Should be: azul_css::props::layout::position::LayoutInsetBottom
        let file_path = PathBuf::from("/project/css/src/props/layout/position.rs");
        let result = index
            .deduce_full_path(&file_path, "LayoutInsetBottom")
            .unwrap();

        assert_eq!(
            result,
            "azul_css::props::layout::position::LayoutInsetBottom"
        );
    }

    #[test]
    fn test_deduce_full_path_lib_rs() {
        let mut crate_names = BTreeMap::new();
        crate_names.insert(PathBuf::from("/project/core"), "azul_core".to_string());

        let index = WorkspaceIndex {
            types: BTreeMap::new(),
            crate_names,
            files: BTreeMap::new(),
            file_sources: BTreeMap::new(),
        };

        // Test: core/src/lib.rs + NodeId
        // Should be: azul_core::NodeId (lib.rs doesn't add to path)
        let file_path = PathBuf::from("/project/core/src/lib.rs");
        let result = index.deduce_full_path(&file_path, "NodeId").unwrap();

        assert_eq!(result, "azul_core::NodeId");
    }

    #[test]
    fn test_deduce_full_path_mod_rs() {
        let mut crate_names = BTreeMap::new();
        crate_names.insert(PathBuf::from("/project/core"), "azul_core".to_string());

        let index = WorkspaceIndex {
            types: BTreeMap::new(),
            crate_names,
            files: BTreeMap::new(),
            file_sources: BTreeMap::new(),
        };

        // Test: core/src/callbacks/mod.rs + Callback
        // Should be: azul_core::callbacks::Callback (mod.rs doesn't add to path)
        let file_path = PathBuf::from("/project/core/src/callbacks/mod.rs");
        let result = index.deduce_full_path(&file_path, "Callback").unwrap();

        assert_eq!(result, "azul_core::callbacks::Callback");
    }

    #[test]
    fn test_deduce_full_path_nested_module() {
        let mut crate_names = BTreeMap::new();
        crate_names.insert(PathBuf::from("/project/dll"), "azul_dll".to_string());

        let index = WorkspaceIndex {
            types: BTreeMap::new(),
            crate_names,
            files: BTreeMap::new(),
            file_sources: BTreeMap::new(),
        };

        // Test: dll/src/widgets/button.rs + ButtonOnClick
        // Should be: azul_dll::widgets::button::ButtonOnClick
        let file_path = PathBuf::from("/project/dll/src/widgets/button.rs");
        let result = index.deduce_full_path(&file_path, "ButtonOnClick").unwrap();

        assert_eq!(result, "azul_dll::widgets::button::ButtonOnClick");
    }

    #[test]
    fn test_deduce_full_path_with_hyphen() {
        let mut crate_names = BTreeMap::new();
        // Crate name with hyphen is normalized to underscore
        crate_names.insert(PathBuf::from("/project/azul-css"), "azul_css".to_string());

        let index = WorkspaceIndex {
            types: BTreeMap::new(),
            crate_names,
            files: BTreeMap::new(),
            file_sources: BTreeMap::new(),
        };

        let file_path = PathBuf::from("/project/azul-css/src/color.rs");
        let result = index.deduce_full_path(&file_path, "ColorU").unwrap();

        assert_eq!(result, "azul_css::color::ColorU");
    }
}
