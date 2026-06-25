//! Priority queue types and scoring heuristics for font build jobs.
//!
//! The async font registry uses a priority queue to process the most
//! important font files first. This module contains:
//!
//! - [`Priority`] levels and [`FcBuildJob`] queue entries
//! - Heuristics for assigning initial priority (scout phase)
//! - Path lookup helpers that deduplicate the "find all known paths
//!   matching a family" pattern used throughout the registry

use alloc::collections::btree_map::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use std::collections::HashSet;
use std::path::PathBuf;

use crate::config;
use crate::utils::normalize_family_name;
use crate::FcPattern;

// ── Priority Queue Types ────────────────────────────────────────────────────

/// Priority levels for font build jobs.
///
/// Higher numeric value = higher priority. The builder pool always
/// processes the highest-priority job first.
///
/// # Heuristics
///
/// - **Low**: Default for all fonts discovered by the scout thread.
/// - **Medium**: Reserved for disk cache hits (cheap deserialization).
/// - **High**: Common OS default fonts (e.g. Arial, Segoe UI, SF Pro) that
///   are very likely to be needed early. Identified by token-matching
///   filenames against [`config::common_font_families`].
/// - **Critical**: The main thread is blocked waiting for this font.
///   Set by [`FcFontRegistry::request_fonts`] when a layout pass needs
///   a font that hasn't been parsed yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Priority {
    /// Everything else found by Scout
    Low = 0,
    /// Disk cache hit (cheap deserialization)
    Medium = 1,
    /// Common OS default fonts (sans-serif, serif, monospace)
    High = 2,
    /// Main thread is blocked waiting for this font
    Critical = 3,
}

/// A job for the Builder pool to process.
///
/// Created by the scout thread (with High/Low priority) or by
/// `request_fonts` (with Critical priority when the main thread is blocked).
/// The builder pool pops the highest-priority job first.
#[derive(Debug, Clone)]
pub struct FcBuildJob {
    /// How urgently this font file needs to be parsed.
    pub priority: Priority,
    /// Absolute path to the font file on disk.
    pub path: PathBuf,
    /// Face index within the font file (for `.ttc` collections).
    /// `None` means parse all faces.
    pub font_index: Option<usize>,
    /// Normalized family name guessed from the filename (lowercase, no separators).
    /// Used for deduplication and priority boosting lookups.
    pub guessed_family: String,
}

impl PartialEq for FcBuildJob {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.path == other.path
    }
}
impl Eq for FcBuildJob {}

impl PartialOrd for FcBuildJob {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for FcBuildJob {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.priority.cmp(&other.priority)
    }
}

// ── Scout Priority Assignment ───────────────────────────────────────────────

/// Assign initial priority for a font file discovered by the scout.
///
/// # Heuristic
///
/// Tokenizes the filename and checks for overlap with common OS font
/// families (e.g. "Helvetica Neue", "Segoe UI", "DejaVu Sans").
/// If any common family's tokens are a substring of the file's tokens
/// (joined, case-insensitive), the font gets **High** priority.
/// Everything else gets **Low**.
pub fn assign_scout_priority(
    file_tokens: &[String],
    common_token_sets: &[Vec<String>],
) -> Priority {
    if config::matches_common_family_tokens(file_tokens, common_token_sets) {
        Priority::High
    } else {
        Priority::Low
    }
}

// ── Path Lookup Helpers ─────────────────────────────────────────────────────

/// Find all known file paths that match a normalized family name.
///
/// # Matching strategy
///
/// 1. **Exact match**: `known_paths[family]` — O(log n) BTreeMap lookup.
/// 2. **Fuzzy match**: For every entry in `known_paths`, check bidirectional
///    substring containment (`known_fam.contains(family) || family.contains(known_fam)`).
///    This catches cases like family="arial" matching known_fam="arialnarrow",
///    or family="notosans" matching known_fam="notosanscjk".
///
/// Returns deduplicated paths (no duplicates from exact + fuzzy overlap).
pub fn find_family_paths(
    family: &str,
    known_paths: &BTreeMap<String, Vec<PathBuf>>,
) -> Vec<PathBuf> {
    let mut result = HashSet::new();

    // Exact match
    if let Some(paths) = known_paths.get(family) {
        result.extend(paths.iter().cloned());
    }

    // Fuzzy substring match
    for (known_fam, paths) in known_paths.iter() {
        if known_fam != family
            && (known_fam.contains(family) || family.contains(known_fam.as_str()))
        {
            result.extend(paths.iter().cloned());
        }
    }

    result.into_iter().collect()
}

/// Find paths for `families` that haven't been fully parsed yet.
///
/// Uses [`find_family_paths`] for lookup, then filters against `completed_paths`
/// to return only those still in progress.
pub fn find_incomplete_paths(
    families: &[String],
    known_paths: &BTreeMap<String, Vec<PathBuf>>,
    completed_paths: &HashSet<PathBuf>,
) -> Vec<(PathBuf, String)> {
    families
        .iter()
        .flat_map(|family| {
            find_family_paths(family, known_paths)
                .into_iter()
                .filter(|p| !completed_paths.contains(p))
                .map(move |p| (p, family.clone()))
        })
        .collect()
}

/// Check whether a normalized family name exists in the pattern cache.
///
/// Matches against both `pattern.name` and `pattern.family` fields,
/// normalizing each for comparison.
pub fn family_exists_in_patterns<'a>(
    family: &str,
    patterns: impl Iterator<Item = &'a FcPattern>,
) -> bool {
    patterns.into_iter().any(|p| {
        p.name
            .as_ref()
            .map(|n| normalize_family_name(n) == family)
            .unwrap_or(false)
            || p.family
                .as_ref()
                .map(|f| normalize_family_name(f) == family)
                .unwrap_or(false)
    })
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Priority ordering ───────────────────────────────────────────────

    #[test]
    fn priority_ordering() {
        assert!(Priority::Critical > Priority::High);
        assert!(Priority::High > Priority::Medium);
        assert!(Priority::Medium > Priority::Low);
    }

    #[test]
    fn build_job_sorts_by_priority() {
        let low = FcBuildJob {
            priority: Priority::Low,
            path: PathBuf::from("a.ttf"),
            font_index: None,
            guessed_family: "a".into(),
        };
        let critical = FcBuildJob {
            priority: Priority::Critical,
            path: PathBuf::from("b.ttf"),
            font_index: None,
            guessed_family: "b".into(),
        };
        let mut jobs = vec![low.clone(), critical.clone()];
        jobs.sort();
        assert_eq!(jobs[0].priority, Priority::Low);
        assert_eq!(jobs[1].priority, Priority::Critical);
    }

    // ── Scout priority assignment ───────────────────────────────────────

    #[test]
    fn assign_scout_priority_common_family_gets_high() {
        use crate::OperatingSystem;
        let common = config::tokenize_common_families(OperatingSystem::MacOS);

        // "Arial" is a common macOS font
        let tokens = tokenize_all("Arial");
        assert_eq!(assign_scout_priority(&tokens, &common), Priority::High);

        // "HelveticaNeue-Bold" contains "Helvetica Neue"
        let tokens = tokenize_all("HelveticaNeue-Bold");
        assert_eq!(assign_scout_priority(&tokens, &common), Priority::High);
    }

    #[test]
    fn assign_scout_priority_unknown_font_gets_low() {
        use crate::OperatingSystem;
        let common = config::tokenize_common_families(OperatingSystem::MacOS);

        let tokens = tokenize_all("SomeObscureFont");
        assert_eq!(assign_scout_priority(&tokens, &common), Priority::Low);
    }

    // ── find_family_paths ───────────────────────────────────────────────

    #[test]
    fn find_family_paths_exact_match() {
        let mut known = BTreeMap::new();
        known.insert("arial".to_string(), vec![PathBuf::from("/fonts/Arial.ttf")]);
        known.insert("helvetica".to_string(), vec![PathBuf::from("/fonts/Helvetica.ttf")]);

        let paths = find_family_paths("arial", &known);
        assert_eq!(paths.len(), 1);
        assert!(paths.contains(&PathBuf::from("/fonts/Arial.ttf")));
    }

    #[test]
    fn find_family_paths_fuzzy_substring() {
        let mut known = BTreeMap::new();
        known.insert("arial".to_string(), vec![PathBuf::from("/fonts/Arial.ttf")]);
        known.insert(
            "arialnarrow".to_string(),
            vec![PathBuf::from("/fonts/ArialNarrow.ttf")],
        );
        known.insert("helvetica".to_string(), vec![PathBuf::from("/fonts/Helvetica.ttf")]);

        // "arial" matches both "arial" (exact) and "arialnarrow" (contains)
        let paths = find_family_paths("arial", &known);
        assert_eq!(paths.len(), 2);
        assert!(paths.contains(&PathBuf::from("/fonts/Arial.ttf")));
        assert!(paths.contains(&PathBuf::from("/fonts/ArialNarrow.ttf")));
    }

    #[test]
    fn find_family_paths_no_match() {
        let mut known = BTreeMap::new();
        known.insert("arial".to_string(), vec![PathBuf::from("/fonts/Arial.ttf")]);

        let paths = find_family_paths("courier", &known);
        assert!(paths.is_empty());
    }

    // ── find_incomplete_paths ───────────────────────────────────────────

    #[test]
    fn find_incomplete_paths_filters_completed() {
        let mut known = BTreeMap::new();
        known.insert(
            "arial".to_string(),
            vec![
                PathBuf::from("/fonts/Arial.ttf"),
                PathBuf::from("/fonts/ArialBold.ttf"),
            ],
        );

        let mut completed = HashSet::new();
        completed.insert(PathBuf::from("/fonts/Arial.ttf"));

        let incomplete = find_incomplete_paths(
            &["arial".to_string()],
            &known,
            &completed,
        );

        assert_eq!(incomplete.len(), 1);
        assert_eq!(incomplete[0].0, PathBuf::from("/fonts/ArialBold.ttf"));
    }

    // ── family_exists_in_patterns ───────────────────────────────────────

    #[test]
    fn family_exists_by_name() {
        let pattern = FcPattern {
            name: Some("Arial".to_string()),
            ..Default::default()
        };
        assert!(family_exists_in_patterns("arial", [&pattern].into_iter()));
    }

    #[test]
    fn family_exists_by_family_field() {
        let pattern = FcPattern {
            family: Some("Helvetica Neue".to_string()),
            ..Default::default()
        };
        assert!(family_exists_in_patterns("helveticaneue", [&pattern].into_iter()));
    }

    #[test]
    fn family_not_found() {
        let pattern = FcPattern {
            name: Some("Arial".to_string()),
            ..Default::default()
        };
        assert!(!family_exists_in_patterns("courier", [&pattern].into_iter()));
    }

    /// Helper: tokenize a stem into all lowercase tokens.
    fn tokenize_all(stem: &str) -> Vec<String> {
        crate::config::tokenize_lowercase(stem)
    }
}
