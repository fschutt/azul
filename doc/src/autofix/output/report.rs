//! Report generation
//!
//! This module generates human-readable reports of autofix results.

use crate::autofix::{analysis::ReachabilityAnalysis, patches::PatchSet};

/// A complete report of autofix analysis
#[derive(Debug)]
pub struct Report {
    /// Summary statistics
    pub summary: ReportSummary,
    /// Path corrections to apply
    pub path_corrections: Vec<PathCorrectionEntry>,
    /// Types to remove
    pub types_to_remove: Vec<String>,
    /// Warnings encountered
    pub warnings: Vec<String>,
    /// Errors encountered
    pub errors: Vec<String>,
}

#[derive(Debug)]
pub struct ReportSummary {
    pub total_types_analyzed: usize,
    pub path_corrections_count: usize,
    pub types_to_remove_count: usize,
    pub duplicates_skipped: usize,
    pub warnings_count: usize,
    pub errors_count: usize,
}

#[derive(Debug)]
pub struct PathCorrectionEntry {
    pub type_name: String,
    pub from: String,
    pub to: String,
    pub reason: String,
}

/// Generate a report from analysis results
pub fn generate_report(patches: &PatchSet, reachability: Option<&ReachabilityAnalysis>) -> Report {
    let path_corrections: Vec<PathCorrectionEntry> = patches
        .path_corrections
        .values()
        .map(|c| PathCorrectionEntry {
            type_name: c.type_name.clone(),
            from: c.current_path.clone(),
            to: c.correct_path.clone(),
            reason: c.reason.clone(),
        })
        .collect();

    let types_to_remove: Vec<String> = patches.types_to_remove.iter().cloned().collect();

    let total_types = reachability
        .map(|r| r.reachable.len() + r.unreachable.len())
        .unwrap_or(0);

    Report {
        summary: ReportSummary {
            total_types_analyzed: total_types,
            path_corrections_count: path_corrections.len(),
            types_to_remove_count: types_to_remove.len(),
            duplicates_skipped: patches.duplicates_skipped,
            warnings_count: 0,
            errors_count: 0,
        },
        path_corrections,
        types_to_remove,
        warnings: Vec::new(),
        errors: Vec::new(),
    }
}

impl Report {
    /// Generate a text report
    pub fn to_text(&self) -> String {
        let mut out = String::new();

        out.push_str("=== AUTOFIX REPORT ===\n\n");

        out.push_str(&format!("Summary:\n"));
        out.push_str(&format!(
            "  Types analyzed: {}\n",
            self.summary.total_types_analyzed
        ));
        out.push_str(&format!(
            "  Path corrections: {}\n",
            self.summary.path_corrections_count
        ));
        out.push_str(&format!(
            "  Types to remove: {}\n",
            self.summary.types_to_remove_count
        ));
        out.push_str(&format!(
            "  Duplicates skipped: {}\n",
            self.summary.duplicates_skipped
        ));
        out.push_str("\n");

        if !self.path_corrections.is_empty() {
            out.push_str("Path Corrections:\n");
            for pc in &self.path_corrections {
                out.push_str(&format!("  {} : {} -> {}\n", pc.type_name, pc.from, pc.to));
                out.push_str(&format!("    Reason: {}\n", pc.reason));
            }
            out.push_str("\n");
        }

        if !self.types_to_remove.is_empty() {
            out.push_str("Types to Remove (unused):\n");
            for ty in &self.types_to_remove {
                out.push_str(&format!("  - {}\n", ty));
            }
            out.push_str("\n");
        }

        if !self.warnings.is_empty() {
            out.push_str("Warnings:\n");
            for w in &self.warnings {
                out.push_str(&format!("  ! {}\n", w));
            }
            out.push_str("\n");
        }

        if !self.errors.is_empty() {
            out.push_str("Errors:\n");
            for e in &self.errors {
                out.push_str(&format!("  x {}\n", e));
            }
            out.push_str("\n");
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autofix::patches::PathCorrection;

    #[test]
    fn test_report_generation() {
        let mut patches = PatchSet::new();
        patches.add_path_correction(PathCorrection {
            type_name: "Dom".to_string(),
            current_path: "azul_dll::Dom".to_string(),
            correct_path: "azul_core::dom::Dom".to_string(),
            reason: "blacklisted crate".to_string(),
        });

        let report = generate_report(&patches, None);

        assert_eq!(report.summary.path_corrections_count, 1);
        assert!(!report.path_corrections.is_empty());
    }

    #[test]
    fn test_report_to_text() {
        let mut patches = PatchSet::new();
        patches.add_path_correction(PathCorrection {
            type_name: "Test".to_string(),
            current_path: "old::path".to_string(),
            correct_path: "new::path".to_string(),
            reason: "test".to_string(),
        });

        let report = generate_report(&patches, None);
        let text = report.to_text();

        assert!(text.contains("Path Corrections"));
        assert!(text.contains("Test"));
    }
}
