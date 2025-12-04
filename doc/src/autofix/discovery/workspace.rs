//! Workspace type discovery
//!
//! This module scans the Rust workspace to find all type definitions
//! and their locations.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::fs;

use super::crates::{get_crate_priority, CratePriority, is_crate_blacklisted};

/// Location of a type in the workspace
#[derive(Debug, Clone)]
pub struct TypeLocation {
    /// Full path like "azul_core::dom::Dom"
    pub full_path: String,
    /// Crate name like "azul_core"
    pub crate_name: String,
    /// Module path within crate like "dom"
    pub module_path: String,
    /// File path relative to workspace root
    pub file_path: PathBuf,
    /// Line number where type is defined
    pub line_number: usize,
    /// Priority of this location (lower is better)
    pub priority: CratePriority,
}

/// Index of all types in the workspace
#[derive(Debug, Default)]
pub struct WorkspaceIndex {
    /// Map from type name to all locations where it's defined
    /// (Some types might be defined in multiple crates)
    pub types: HashMap<String, Vec<TypeLocation>>,
    /// Map from full path to type name
    pub path_to_name: HashMap<String, String>,
    /// Errors encountered during indexing
    pub errors: Vec<String>,
}

impl WorkspaceIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the best location for a type (highest priority)
    pub fn get_best_location(&self, type_name: &str) -> Option<&TypeLocation> {
        self.types.get(type_name)
            .and_then(|locs| locs.iter().min_by_key(|l| l.priority))
    }

    /// Get the canonical path for a type (from best location)
    pub fn get_canonical_path(&self, type_name: &str) -> Option<String> {
        self.get_best_location(type_name).map(|l| l.full_path.clone())
    }

    /// Check if a type exists in the workspace
    pub fn has_type(&self, type_name: &str) -> bool {
        self.types.contains_key(type_name)
    }

    /// Get all type names
    pub fn all_type_names(&self) -> impl Iterator<Item = &String> {
        self.types.keys()
    }

    /// Add a type location
    pub fn add_type(&mut self, name: String, location: TypeLocation) {
        let full_path = location.full_path.clone();
        self.types.entry(name.clone()).or_default().push(location);
        self.path_to_name.insert(full_path, name);
    }
}

/// Discover all types in the workspace
pub fn discover_workspace_types(workspace_root: &Path) -> WorkspaceIndex {
    let mut index = WorkspaceIndex::new();

    // Scan each crate directory
    let crate_dirs = [
        ("azul_core", "core/src"),
        ("azul_css", "css/src"),
        ("azul_layout", "layout/src"),
        // Note: azul_dll is not scanned for type discovery
    ];

    for (crate_name, src_path) in &crate_dirs {
        let src_dir = workspace_root.join(src_path);
        if src_dir.exists() {
            scan_crate_directory(&mut index, crate_name, &src_dir, "");
        }
    }

    index
}

/// Scan a crate directory for type definitions
fn scan_crate_directory(
    index: &mut WorkspaceIndex,
    crate_name: &str,
    dir: &Path,
    module_prefix: &str,
) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            index.errors.push(format!("Failed to read {}: {}", dir.display(), e));
            return;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_dir() {
            // Recurse into subdirectory
            let dir_name = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");

            let new_prefix = if module_prefix.is_empty() {
                dir_name.to_string()
            } else {
                format!("{}::{}", module_prefix, dir_name)
            };

            scan_crate_directory(index, crate_name, &path, &new_prefix);
        } else if path.extension().map_or(false, |e| e == "rs") {
            // Parse Rust file for type definitions
            let file_name = path.file_stem()
                .and_then(|n| n.to_str())
                .unwrap_or("");

            // Skip mod.rs for module path calculation
            let module_path = if file_name == "mod" || file_name == "lib" {
                module_prefix.to_string()
            } else if module_prefix.is_empty() {
                file_name.to_string()
            } else {
                format!("{}::{}", module_prefix, file_name)
            };

            scan_rust_file(index, crate_name, &path, &module_path);
        }
    }
}

/// Scan a Rust file for type definitions
fn scan_rust_file(
    index: &mut WorkspaceIndex,
    crate_name: &str,
    file_path: &Path,
    module_path: &str,
) {
    let content = match fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(e) => {
            index.errors.push(format!("Failed to read {}: {}", file_path.display(), e));
            return;
        }
    };

    let priority = get_crate_priority(crate_name);

    // Simple regex-free parsing for type definitions
    // This is faster than syn for large codebases and sufficient for our needs
    for (line_num, line) in content.lines().enumerate() {
        let line = line.trim();

        // Skip comments
        if line.starts_with("//") || line.starts_with("/*") || line.starts_with('*') {
            continue;
        }

        // Look for struct, enum, type, or union definitions
        let type_name = extract_type_name(line);

        if let Some(name) = type_name {
            let full_path = if module_path.is_empty() {
                format!("{}::{}", crate_name, name)
            } else {
                format!("{}::{}::{}", crate_name, module_path, name)
            };

            let location = TypeLocation {
                full_path,
                crate_name: crate_name.to_string(),
                module_path: module_path.to_string(),
                file_path: file_path.to_path_buf(),
                line_number: line_num + 1,
                priority,
            };

            index.add_type(name, location);
        }
    }
}

/// Extract a type name from a line of Rust code
fn extract_type_name(line: &str) -> Option<String> {
    // Remove visibility modifiers
    let line = line
        .trim_start_matches("pub ")
        .trim_start_matches("pub(crate) ")
        .trim_start_matches("pub(super) ")
        .trim_start_matches("pub(self) ");

    // Check for different type definitions
    let name = if let Some(rest) = line.strip_prefix("struct ") {
        extract_identifier(rest)
    } else if let Some(rest) = line.strip_prefix("enum ") {
        extract_identifier(rest)
    } else if let Some(rest) = line.strip_prefix("type ") {
        extract_identifier(rest)
    } else if let Some(rest) = line.strip_prefix("union ") {
        extract_identifier(rest)
    } else {
        None
    };

    // Filter out reserved keywords and obviously wrong matches
    name.filter(|n| !is_reserved_keyword(n) && !n.is_empty())
}

/// Extract a Rust identifier from the start of a string
fn extract_identifier(s: &str) -> Option<String> {
    let s = s.trim();

    // Handle generic types like "Foo<T>" or "Foo<T: Clone>"
    let end = s.find(|c: char| !c.is_alphanumeric() && c != '_')
        .unwrap_or(s.len());

    if end == 0 {
        return None;
    }

    let name = &s[..end];

    // Must start with uppercase for type names (except for type aliases)
    let first = name.chars().next()?;
    if !first.is_ascii_uppercase() && first != '_' {
        return None;
    }

    Some(name.to_string())
}

/// Check if a name is a Rust keyword
fn is_reserved_keyword(name: &str) -> bool {
    matches!(name,
        "Self" | "self" | "super" | "crate" |
        "where" | "impl" | "trait" | "for" |
        "as" | "use" | "mod" | "pub" | "const" |
        "static" | "extern" | "fn" | "let" | "mut" |
        "ref" | "if" | "else" | "match" | "loop" |
        "while" | "for" | "in" | "break" | "continue" |
        "return" | "async" | "await" | "move" | "dyn" |
        "unsafe" | "true" | "false"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_type_name() {
        assert_eq!(extract_type_name("pub struct Foo {"), Some("Foo".to_string()));
        assert_eq!(extract_type_name("struct Bar<T> {"), Some("Bar".to_string()));
        assert_eq!(extract_type_name("pub enum Baz {"), Some("Baz".to_string()));
        assert_eq!(extract_type_name("type MyAlias = i32;"), Some("MyAlias".to_string()));
        assert_eq!(extract_type_name("let x = 5;"), None);
        assert_eq!(extract_type_name("// struct Comment"), None);
    }

    #[test]
    fn test_extract_identifier() {
        assert_eq!(extract_identifier("Foo<T>"), Some("Foo".to_string()));
        assert_eq!(extract_identifier("Bar {"), Some("Bar".to_string()));
        assert_eq!(extract_identifier("_Internal"), Some("_Internal".to_string()));
        assert_eq!(extract_identifier("lowercase"), None);
    }
}
