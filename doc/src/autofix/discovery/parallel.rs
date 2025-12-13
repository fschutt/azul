//! Parallel workspace type discovery
//!
//! This module provides parallelized scanning of the Rust workspace
//! using rayon for maximum performance.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use rayon::prelude::*;

use super::{
    crates::{get_crate_priority, CratePriority},
    workspace::{TypeLocation, WorkspaceIndex},
};

/// Configuration for parallel discovery
#[derive(Debug, Clone)]
pub struct ParallelDiscoveryConfig {
    /// Number of threads to use (0 = auto-detect)
    pub num_threads: usize,
    /// Crate directories to scan
    pub crate_dirs: Vec<(String, PathBuf)>,
}

impl Default for ParallelDiscoveryConfig {
    fn default() -> Self {
        Self {
            num_threads: 0, // Auto-detect
            crate_dirs: vec![
                ("azul_core".to_string(), PathBuf::from("core/src")),
                ("azul_css".to_string(), PathBuf::from("css/src")),
                ("azul_layout".to_string(), PathBuf::from("layout/src")),
            ],
        }
    }
}

/// File info for parallel processing
#[derive(Debug)]
struct FileToScan {
    crate_name: String,
    file_path: PathBuf,
    module_path: String,
}

/// Discovered type from a file
#[derive(Debug)]
struct DiscoveredType {
    name: String,
    location: TypeLocation,
}

/// Discover all types in the workspace using parallel scanning
pub fn discover_workspace_types_parallel(
    workspace_root: &Path,
    config: ParallelDiscoveryConfig,
) -> WorkspaceIndex {
    // Phase 1: Collect all files to scan (single-threaded, fast)
    let files_to_scan = collect_files_to_scan(workspace_root, &config);

    println!(
        "     Scanning {} Rust files in parallel...",
        files_to_scan.len()
    );

    // Phase 2: Scan files in parallel
    let discovered: Vec<DiscoveredType> = files_to_scan
        .par_iter()
        .flat_map(|file_info| scan_file_for_types(file_info))
        .collect();

    // Phase 3: Build index (single-threaded, merging results)
    let mut index = WorkspaceIndex::new();
    for discovered_type in discovered {
        index.add_type(discovered_type.name, discovered_type.location);
    }

    index
}

/// Collect all Rust files to scan from all crate directories
fn collect_files_to_scan(
    workspace_root: &Path,
    config: &ParallelDiscoveryConfig,
) -> Vec<FileToScan> {
    let mut files = Vec::new();

    for (crate_name, src_path) in &config.crate_dirs {
        let src_dir = workspace_root.join(src_path);
        if src_dir.exists() {
            collect_files_recursive(&mut files, crate_name, &src_dir, "");
        }
    }

    files
}

/// Recursively collect all .rs files from a directory
fn collect_files_recursive(
    files: &mut Vec<FileToScan>,
    crate_name: &str,
    dir: &Path,
    module_prefix: &str,
) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_dir() {
            let dir_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            // Skip hidden directories and target
            if dir_name.starts_with('.') || dir_name == "target" {
                continue;
            }

            let new_prefix = if module_prefix.is_empty() {
                dir_name.to_string()
            } else {
                format!("{}::{}", module_prefix, dir_name)
            };

            collect_files_recursive(files, crate_name, &path, &new_prefix);
        } else if path.extension().map_or(false, |e| e == "rs") {
            let file_name = path.file_stem().and_then(|n| n.to_str()).unwrap_or("");

            // Calculate module path
            let module_path = if file_name == "mod" || file_name == "lib" {
                module_prefix.to_string()
            } else if module_prefix.is_empty() {
                file_name.to_string()
            } else {
                format!("{}::{}", module_prefix, file_name)
            };

            files.push(FileToScan {
                crate_name: crate_name.to_string(),
                file_path: path.clone(),
                module_path,
            });
        }
    }
}

/// Scan a single file for type definitions (thread-safe)
fn scan_file_for_types(file_info: &FileToScan) -> Vec<DiscoveredType> {
    let content = match fs::read_to_string(&file_info.file_path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };

    let priority = get_crate_priority(&file_info.crate_name);
    let mut types = Vec::new();

    for (line_num, line) in content.lines().enumerate() {
        if let Some(type_name) = extract_type_name_fast(line) {
            let full_path = if file_info.module_path.is_empty() {
                format!("{}::{}", file_info.crate_name, type_name)
            } else {
                format!(
                    "{}::{}::{}",
                    file_info.crate_name, file_info.module_path, type_name
                )
            };

            types.push(DiscoveredType {
                name: type_name,
                location: TypeLocation {
                    full_path,
                    crate_name: file_info.crate_name.clone(),
                    module_path: file_info.module_path.clone(),
                    file_path: file_info.file_path.clone(),
                    line_number: line_num + 1,
                    priority,
                },
            });
        }
    }

    types
}

/// Fast type name extraction without regex
///
/// This is optimized for parallel scanning - no allocations except for the result.
fn extract_type_name_fast(line: &str) -> Option<String> {
    let line = line.trim();

    // Skip comments
    if line.starts_with("//") || line.starts_with("/*") || line.starts_with('*') {
        return None;
    }

    // Remove visibility modifiers (in-place view)
    let line = line
        .strip_prefix("pub(crate) ")
        .or_else(|| line.strip_prefix("pub(super) "))
        .or_else(|| line.strip_prefix("pub(self) "))
        .or_else(|| line.strip_prefix("pub "))
        .unwrap_or(line);

    // Check for type definitions
    let rest = line
        .strip_prefix("struct ")
        .or_else(|| line.strip_prefix("enum "))
        .or_else(|| line.strip_prefix("type "))
        .or_else(|| line.strip_prefix("union "))?;

    // Extract identifier
    let rest = rest.trim();
    let end = rest
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .unwrap_or(rest.len());

    if end == 0 {
        return None;
    }

    let name = &rest[..end];

    // Must start with uppercase (type names)
    let first = name.chars().next()?;
    if !first.is_ascii_uppercase() && first != '_' {
        return None;
    }

    // Filter reserved keywords
    if is_reserved(name) {
        return None;
    }

    Some(name.to_string())
}

/// Check if a name is a Rust keyword (fast inline check)
#[inline]
fn is_reserved(name: &str) -> bool {
    matches!(
        name,
        "Self"
            | "self"
            | "super"
            | "crate"
            | "where"
            | "impl"
            | "trait"
            | "for"
            | "as"
            | "use"
            | "mod"
            | "pub"
            | "const"
            | "static"
            | "extern"
            | "fn"
            | "let"
            | "mut"
            | "ref"
            | "if"
            | "else"
            | "match"
            | "loop"
            | "while"
            | "break"
            | "continue"
            | "return"
            | "async"
            | "await"
            | "move"
            | "dyn"
            | "unsafe"
            | "true"
            | "false"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_type_name_fast() {
        assert_eq!(
            extract_type_name_fast("pub struct Foo {"),
            Some("Foo".to_string())
        );
        assert_eq!(
            extract_type_name_fast("struct Bar<T> {"),
            Some("Bar".to_string())
        );
        assert_eq!(
            extract_type_name_fast("pub(crate) enum Baz {"),
            Some("Baz".to_string())
        );
        assert_eq!(
            extract_type_name_fast("type MyAlias = i32;"),
            Some("MyAlias".to_string())
        );
        assert_eq!(extract_type_name_fast("// struct Comment"), None);
        assert_eq!(extract_type_name_fast("let x = 5;"), None);
        assert_eq!(extract_type_name_fast("fn foo() {}"), None);
    }

    #[test]
    fn test_parallel_safety() {
        // Ensure our types implement Send + Sync for parallel processing
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}

        assert_send::<FileToScan>();
        assert_sync::<FileToScan>();
        assert_send::<DiscoveredType>();
        assert_sync::<DiscoveredType>();
    }
}
