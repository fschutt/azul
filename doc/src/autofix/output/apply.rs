//! Patch application
//!
//! This module handles applying patches to api.json.

use std::{fs, path::Path};

use crate::autofix::patches::PatchSet;

/// Result of applying patches
#[derive(Debug)]
pub struct ApplyResult {
    /// Number of path corrections applied
    pub path_corrections_applied: usize,
    /// Number of types removed
    pub types_removed: usize,
    /// Errors encountered during application
    pub errors: Vec<String>,
    /// Whether the file was modified
    pub modified: bool,
}

/// Apply patches to api.json
pub fn apply_patches(
    api_json_path: &Path, 
    patches: &PatchSet, 
    dry_run: bool
) -> ApplyResult {
    
    let mut result = ApplyResult {
        path_corrections_applied: 0,
        types_removed: 0,
        errors: Vec::new(),
        modified: false,
    };

    // Read the api.json file
    let content = match fs::read_to_string(api_json_path) {
        Ok(c) => c,
        Err(e) => {
            result
                .errors
                .push(format!("Failed to read api.json: {}", e));
            return result;
        }
    };

    // Parse as JSON
    let mut api: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            result
                .errors
                .push(format!("Failed to parse api.json: {}", e));
            return result;
        }
    };

    let mut modified = false;

    // Apply path corrections to "external" fields
    if let Some(classes) = api.get_mut("classes").and_then(|v| v.as_object_mut()) {
        for (type_name, class_def) in classes.iter_mut() {
            // Check for path correction
            if let Some(correction) = patches.path_corrections.get(type_name) {
                if let Some(class_obj) = class_def.as_object_mut() {
                    if let Some(ext) = class_obj.get_mut("external") {
                        if ext.as_str() == Some(&correction.current_path) {
                            *ext = serde_json::Value::String(correction.correct_path.clone());
                            result.path_corrections_applied += 1;
                            modified = true;
                        }
                    }
                }
            }
        }

        // Remove unused types
        let types_to_remove: Vec<String> = patches.types_to_remove.iter().cloned().collect();

        for type_name in types_to_remove {
            if classes.remove(&type_name).is_some() {
                result.types_removed += 1;
                modified = true;
            }
        }
    }

    result.modified = modified;

    if modified && !dry_run {
        // Write back the modified JSON
        let output = serde_json::to_string_pretty(&api).unwrap_or_else(|e| {
            result
                .errors
                .push(format!("Failed to serialize api.json: {}", e));
            String::new()
        });

        if !output.is_empty() {
            if let Err(e) = fs::write(api_json_path, output) {
                result
                    .errors
                    .push(format!("Failed to write api.json: {}", e));
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::NamedTempFile;

    use super::*;
    use crate::autofix::patches::PathCorrection;

    #[test]
    fn test_dry_run() {
        let mut patches = PatchSet::new();
        patches.add_path_correction(PathCorrection {
            type_name: "Test".to_string(),
            current_path: "old::path".to_string(),
            correct_path: "new::path".to_string(),
            reason: "test".to_string(),
        });

        // Create a temp file
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"{{"classes": {{"Test": {{"external": "old::path"}}}}}}"#
        )
        .unwrap();

        let result = apply_patches(file.path(), &patches, true);

        // Dry run should report changes but not modify
        assert!(result.modified);
        assert!(result.errors.is_empty());

        // File should be unchanged
        let content = fs::read_to_string(file.path()).unwrap();
        assert!(content.contains("old::path"));
    }
}
