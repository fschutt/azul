//! Path correction for api.json types
//!
//! This module generates patches to correct type paths in api.json
//! to match the actual locations in the workspace.

use std::collections::BTreeMap;

use crate::autofix::discovery::{crates::is_path_blacklisted, TypeLocation, WorkspaceIndex};

/// A path correction patch
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PathCorrection {
    /// The type name (e.g., "Dom")
    pub type_name: String,
    /// Current path in api.json (e.g., "azul_dll::widgets::Dom")
    pub current_path: String,
    /// Correct path from workspace (e.g., "azul_core::dom::Dom")
    pub correct_path: String,
    /// Reason for the correction
    pub reason: String,
}

/// Generate path corrections for all types in api.json
pub fn generate_path_corrections(
    api: &serde_json::Value,
    workspace: &WorkspaceIndex,
) -> Vec<PathCorrection> {
    let mut corrections = Vec::new();

    if let Some(classes) = api.get("classes").and_then(|v| v.as_object()) {
        for (type_name, class_def) in classes {
            // Check external path
            if let Some(ext) = class_def.get("external").and_then(|v| v.as_str()) {
                if let Some(correction) = check_path(type_name, ext, workspace) {
                    corrections.push(correction);
                }
            }

            // Check struct fields
            if let Some(fields) = class_def.get("struct_fields").and_then(|v| v.as_object()) {
                for (_field_name, field_type) in fields {
                    if let Some(type_str) = field_type.as_str() {
                        // Check if the type string contains a path
                        if let Some(correction) = check_type_string(type_str, workspace) {
                            corrections.push(correction);
                        }
                    }
                }
            }

            // Check enum variants
            if let Some(variants) = class_def.get("enum_fields").and_then(|v| v.as_object()) {
                for (_variant_name, variant_type) in variants {
                    if let Some(type_str) = variant_type.as_str() {
                        if !type_str.is_empty() {
                            if let Some(correction) = check_type_string(type_str, workspace) {
                                corrections.push(correction);
                            }
                        }
                    }
                }
            }
        }
    }

    corrections
}

/// Check if a path needs correction
fn check_path(
    type_name: &str,
    current_path: &str,
    workspace: &WorkspaceIndex,
) -> Option<PathCorrection> {
    // Skip if current path is from a blacklisted crate
    let is_blacklisted = is_path_blacklisted(current_path);

    // Get the correct path from workspace
    let correct_path = workspace.get_canonical_path(type_name)?;

    // If paths differ, or current is blacklisted, we need a correction
    if current_path != correct_path {
        let reason = if is_blacklisted {
            format!("path uses blacklisted crate: {}", current_path)
        } else {
            format!("path mismatch: {} vs {}", current_path, correct_path)
        };

        Some(PathCorrection {
            type_name: type_name.to_string(),
            current_path: current_path.to_string(),
            correct_path,
            reason,
        })
    } else {
        None
    }
}

/// Check a type string for embedded paths that need correction
fn check_type_string(type_str: &str, workspace: &WorkspaceIndex) -> Option<PathCorrection> {
    // For now, just check if the string contains a full path
    // More sophisticated parsing would be needed for generics like Option<azul_dll::Foo>

    // Simple check: if it contains :: and starts with a crate name
    if type_str.contains("::") {
        let parts: Vec<&str> = type_str.split("::").collect();
        if parts.len() >= 2 {
            let crate_name = parts[0];
            if is_path_blacklisted(type_str) {
                // Try to find the type name (last part)
                let type_name = parts.last().unwrap();
                if let Some(correct) = workspace.get_canonical_path(type_name) {
                    return Some(PathCorrection {
                        type_name: type_name.to_string(),
                        current_path: type_str.to_string(),
                        correct_path: correct,
                        reason: format!("uses blacklisted crate {}", crate_name),
                    });
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::autofix::discovery::crates::CratePriority;

    fn make_test_workspace() -> WorkspaceIndex {
        let mut ws = WorkspaceIndex::new();

        ws.add_type(
            "Dom".to_string(),
            TypeLocation {
                full_path: "azul_core::dom::Dom".to_string(),
                crate_name: "azul_core".to_string(),
                module_path: "dom".to_string(),
                file_path: PathBuf::from("core/src/dom.rs"),
                line_number: 100,
                priority: CratePriority::Primary,
            },
        );

        ws.add_type(
            "Button".to_string(),
            TypeLocation {
                full_path: "azul_layout::widgets::button::Button".to_string(),
                crate_name: "azul_layout".to_string(),
                module_path: "widgets::button".to_string(),
                file_path: PathBuf::from("layout/src/widgets/button.rs"),
                line_number: 50,
                priority: CratePriority::Secondary,
            },
        );

        ws
    }

    #[test]
    fn test_correct_blacklisted_path() {
        let ws = make_test_workspace();

        let correction = check_path("Dom", "azul_dll::dom::Dom", &ws);
        assert!(correction.is_some());

        let c = correction.unwrap();
        assert_eq!(c.correct_path, "azul_core::dom::Dom");
    }

    #[test]
    fn test_no_correction_needed() {
        let ws = make_test_workspace();

        let correction = check_path("Dom", "azul_core::dom::Dom", &ws);
        assert!(correction.is_none());
    }
}
