//! Patch deduplication
//!
//! This module handles deduplicating patches to avoid multiple
//! corrections for the same issue.

use std::collections::{BTreeMap, BTreeSet};

use super::path_correction::PathCorrection;

/// A set of deduplicated patches
#[derive(Debug, Default)]
pub struct PatchSet {
    /// Path corrections, keyed by type name
    pub path_corrections: BTreeMap<String, PathCorrection>,
    /// Types to remove (unused types)
    pub types_to_remove: BTreeSet<String>,
    /// Duplicate entries that were skipped
    pub duplicates_skipped: usize,
}

impl PatchSet {
    pub fn new() -> Self {
        Self::default()
    }

    /// Total number of patches
    pub fn total_patches(&self) -> usize {
        self.path_corrections.len() + self.types_to_remove.len()
    }

    /// Add a path correction, deduplicating by type name
    pub fn add_path_correction(&mut self, correction: PathCorrection) {
        if self.path_corrections.contains_key(&correction.type_name) {
            self.duplicates_skipped += 1;
        } else {
            self.path_corrections
                .insert(correction.type_name.clone(), correction);
        }
    }

    /// Add a type to remove
    pub fn add_type_to_remove(&mut self, type_name: String) {
        self.types_to_remove.insert(type_name);
    }
}

/// Deduplicate a list of path corrections
pub fn deduplicate_patches(corrections: Vec<PathCorrection>) -> PatchSet {
    let mut patch_set = PatchSet::new();

    for correction in corrections {
        patch_set.add_path_correction(correction);
    }

    patch_set
}

/// Validate patches for consistency
pub fn validate_patches(patches: &PatchSet) -> Vec<String> {
    let mut errors = Vec::new();

    // Check that we're not removing a type that also has a path correction
    for type_name in &patches.types_to_remove {
        if patches.path_corrections.contains_key(type_name) {
            errors.push(format!(
                "Inconsistency: '{}' marked for removal but also has path correction",
                type_name
            ));
        }
    }

    // Check for self-referential corrections
    for (type_name, correction) in &patches.path_corrections {
        if correction.current_path == correction.correct_path {
            errors.push(format!(
                "No-op correction for '{}': {} -> {} (same path)",
                type_name, correction.current_path, correction.correct_path
            ));
        }
    }

    errors
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deduplication() {
        let corrections = vec![
            PathCorrection {
                type_name: "Dom".to_string(),
                current_path: "azul_dll::Dom".to_string(),
                correct_path: "azul_core::dom::Dom".to_string(),
                reason: "first".to_string(),
            },
            PathCorrection {
                type_name: "Dom".to_string(),
                current_path: "azul_dll::Dom".to_string(),
                correct_path: "azul_core::dom::Dom".to_string(),
                reason: "duplicate".to_string(),
            },
        ];

        let patches = deduplicate_patches(corrections);

        assert_eq!(patches.path_corrections.len(), 1);
        assert_eq!(patches.duplicates_skipped, 1);
    }

    #[test]
    fn test_validate_no_op() {
        let mut patches = PatchSet::new();
        patches.add_path_correction(PathCorrection {
            type_name: "Foo".to_string(),
            current_path: "same::path".to_string(),
            correct_path: "same::path".to_string(),
            reason: "no-op".to_string(),
        });

        let errors = validate_patches(&patches);
        assert!(!errors.is_empty());
    }
}
