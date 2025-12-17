//! Fallback type location strategies for when normal locate_source fails
//!
//! This module provides workspace-wide search strategies as a last resort
//! for finding type definitions, particularly useful for callback types
//! and other types that may not be found through cargo metadata.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use syn::{File, Item};

/// Cache for Cargo.toml crate names mapped to their directories
type CrateNameCache = BTreeMap<PathBuf, String>;

/// Search the entire workspace for a type definition by walking all .rs files
///
/// This is a fallback strategy that:
/// 1. Walks all directories from project_root
/// 2. Skips /target, .git, and other build artifacts
/// 3. Finds all `src` directories
/// 4. Parses Cargo.toml to get crate names
/// 5. Searches for the type definition in .rs files
/// 6. Reconstructs the full type path (e.g., azul_core::callbacks::TypeName)
pub fn find_type_in_workspace(project_root: &Path, type_name: &str) -> Result<(String, String)> {
    println!(
        "    [SEARCH] Fallback: searching workspace for type '{}'...",
        type_name
    );

    // Build crate name cache
    let crate_cache = build_crate_name_cache(project_root)?;

    // Find all src directories
    let src_dirs = find_src_directories(project_root)?;

    println!(
        "    [DIR] Found {} src directories to search",
        src_dirs.len()
    );

    // Search each src directory for the type
    for src_dir in src_dirs {
        if let Ok((source_code, file_path)) = search_directory_for_type(&src_dir, type_name) {
            // Reconstruct the full type path
            if let Ok(type_path) =
                reconstruct_type_path(&file_path, &src_dir, type_name, &crate_cache)
            {
                println!("    [OK] Found {} at {}", type_name, type_path);
                return Ok((source_code, type_path));
            }
        }
    }

    anyhow::bail!("Type '{}' not found in workspace", type_name)
}

/// Build a cache mapping crate directories to their crate names
fn build_crate_name_cache(project_root: &Path) -> Result<CrateNameCache> {
    let mut cache = BTreeMap::new();

    // Find all Cargo.toml files
    let cargo_tomls = find_cargo_tomls(project_root)?;

    for toml_path in cargo_tomls {
        if let Ok(content) = fs::read_to_string(&toml_path) {
            // Parse Cargo.toml to extract package.name
            if let Ok(toml_value) = content.parse::<toml::Value>() {
                if let Some(package) = toml_value.get("package") {
                    if let Some(name) = package.get("name") {
                        if let Some(name_str) = name.as_str() {
                            let crate_dir = toml_path.parent().unwrap().to_path_buf();
                            // Normalize crate name: replace '-' with '_'
                            let normalized_name = name_str.replace('-', "_");
                            cache.insert(crate_dir, normalized_name);
                        }
                    }
                }
            }
        }
    }

    Ok(cache)
}

/// Find all Cargo.toml files in the workspace
fn find_cargo_tomls(root: &Path) -> Result<Vec<PathBuf>> {
    let mut tomls = Vec::new();
    visit_dirs(root, &mut |path| {
        if path.file_name() == Some(std::ffi::OsStr::new("Cargo.toml")) {
            tomls.push(path.to_path_buf());
        }
    })?;
    Ok(tomls)
}

/// Find all 'src' directories in the workspace
fn find_src_directories(root: &Path) -> Result<Vec<PathBuf>> {
    let mut src_dirs = Vec::new();
    visit_dirs(root, &mut |path| {
        if path.is_dir() && path.file_name() == Some(std::ffi::OsStr::new("src")) {
            src_dirs.push(path.to_path_buf());
        }
    })?;
    Ok(src_dirs)
}

/// Visit all directories recursively, skipping build artifacts
fn visit_dirs<F>(dir: &Path, cb: &mut F) -> Result<()>
where
    F: FnMut(&Path),
{
    if !dir.is_dir() {
        return Ok(());
    }

    // Skip common build/cache directories
    if let Some(name) = dir.file_name() {
        let name_str = name.to_string_lossy();
        if name_str == "target"
            || name_str == ".git"
            || name_str == "node_modules"
            || name_str == ".cargo"
            || name_str == "build"
        {
            return Ok(());
        }
    }

    cb(dir);

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            visit_dirs(&path, cb)?;
        } else {
            cb(&path);
        }
    }

    Ok(())
}

/// Search a directory for a type definition
///
/// Returns the source code of the file containing the type and the file path
fn search_directory_for_type(src_dir: &Path, type_name: &str) -> Result<(String, PathBuf)> {
    let mut rs_files = Vec::new();

    // Collect all .rs files in this src directory
    visit_dirs(src_dir, &mut |path| {
        if path.is_file() && path.extension() == Some(std::ffi::OsStr::new("rs")) {
            rs_files.push(path.to_path_buf());
        }
    })?;

    // Search each .rs file
    for rs_file in rs_files {
        if let Ok(content) = fs::read_to_string(&rs_file) {
            // Quick pre-check: does the file contain the type name?
            if !content.contains(type_name) {
                continue;
            }

            // Parse and check for the actual definition
            if let Ok(syntax_tree) = syn::parse_file(&content) {
                if file_contains_type(&syntax_tree, type_name) {
                    return Ok((content, rs_file));
                }
            }
        }
    }

    anyhow::bail!("Type '{}' not found in {}", type_name, src_dir.display())
}

/// Check if a parsed file contains a type definition
fn file_contains_type(syntax_tree: &File, type_name: &str) -> bool {
    for item in &syntax_tree.items {
        match item {
            Item::Struct(s) if s.ident == type_name => return true,
            Item::Enum(e) if e.ident == type_name => return true,
            Item::Type(t) if t.ident == type_name => return true,
            _ => {}
        }
    }
    false
}

/// Reconstruct the full type path from a file location
///
/// Example:
/// - File: /path/to/azul/core/src/callbacks.rs
/// - Crate: azul_core (from Cargo.toml in /path/to/azul/core)
/// - Type: MarshaledLayoutCallbackType
/// - Result: azul_core::callbacks::MarshaledLayoutCallbackType
fn reconstruct_type_path(
    file_path: &Path,
    src_dir: &Path,
    type_name: &str,
    crate_cache: &CrateNameCache,
) -> Result<String> {
    // Find the crate directory (parent of src_dir)
    let crate_dir = src_dir.parent().context("src directory has no parent")?;

    // Get crate name from cache
    let crate_name = crate_cache
        .get(crate_dir)
        .context(format!("No crate name found for {:?}", crate_dir))?;

    // Get the relative path from src_dir to file_path
    let rel_path = file_path
        .strip_prefix(src_dir)
        .context("File is not in src directory")?;

    // Build module path from file path
    // e.g., callbacks.rs -> callbacks
    // e.g., dom/node.rs -> dom::node
    let mut module_parts = Vec::new();

    for component in rel_path.components() {
        if let Some(part) = component.as_os_str().to_str() {
            // Remove .rs extension
            let part_clean = part.strip_suffix(".rs").unwrap_or(part);

            // Skip lib.rs and mod.rs - they don't add to the module path
            if part_clean == "lib" || part_clean == "mod" {
                continue;
            }

            module_parts.push(part_clean.to_string());
        }
    }

    // Construct full path: crate_name::module::path::TypeName
    let mut full_path = crate_name.clone();
    for part in module_parts {
        full_path.push_str("::");
        full_path.push_str(&part);
    }
    full_path.push_str("::");
    full_path.push_str(type_name);

    Ok(full_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crate_name_normalization() {
        let cache = BTreeMap::from([(PathBuf::from("/test/azul-core"), "azul_core".to_string())]);
        assert_eq!(
            cache.get(&PathBuf::from("/test/azul-core")).unwrap(),
            "azul_core"
        );
    }
}
