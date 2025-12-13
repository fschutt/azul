//! Crate priority configuration
//!
//! This module defines which crates should be used for type resolution
//! and their priority order.

use std::collections::HashMap;

/// Priority level for a crate
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CratePriority {
    /// Highest priority - use this crate first
    Primary = 0,
    /// Secondary priority - use if not found in primary
    Secondary = 1,
    /// Tertiary priority
    Tertiary = 2,
    /// Should not be used for type resolution (e.g., azul_dll)
    Blacklisted = 99,
}

/// Crate priorities for the azul workspace
///
/// This defines which crates should be searched for type definitions
/// and in what order. azul_dll is blacklisted because memtest cannot
/// depend on it (circular dependency issue).
pub static CRATE_PRIORITIES: &[(&str, CratePriority)] = &[
    // Primary crates - search first
    ("azul_core", CratePriority::Primary),
    ("azul_css", CratePriority::Primary),
    // Secondary - search after primary
    ("azul_layout", CratePriority::Secondary),
    // Blacklisted - never use for type resolution
    ("azul_dll", CratePriority::Blacklisted),
];

/// Get priority for a crate
pub fn get_crate_priority(crate_name: &str) -> CratePriority {
    for (name, priority) in CRATE_PRIORITIES {
        if *name == crate_name {
            return *priority;
        }
    }
    // Unknown crates get tertiary priority
    CratePriority::Tertiary
}

/// Check if a crate is blacklisted
pub fn is_crate_blacklisted(crate_name: &str) -> bool {
    get_crate_priority(crate_name) == CratePriority::Blacklisted
}

/// Get ordered list of crates to search (excludes blacklisted)
pub fn get_search_order() -> Vec<&'static str> {
    let mut crates: Vec<_> = CRATE_PRIORITIES
        .iter()
        .filter(|(_, p)| *p != CratePriority::Blacklisted)
        .collect();

    crates.sort_by_key(|(_, p)| *p);

    crates.into_iter().map(|(name, _)| *name).collect()
}

/// Extract crate name from a full path like "azul_core::dom::Dom"
pub fn extract_crate_from_path(path: &str) -> Option<&str> {
    path.split("::").next()
}

/// Check if a path is from a blacklisted crate
pub fn is_path_blacklisted(path: &str) -> bool {
    if let Some(crate_name) = extract_crate_from_path(path) {
        is_crate_blacklisted(crate_name)
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crate_priority() {
        assert_eq!(get_crate_priority("azul_core"), CratePriority::Primary);
        assert_eq!(get_crate_priority("azul_dll"), CratePriority::Blacklisted);
        assert_eq!(get_crate_priority("unknown"), CratePriority::Tertiary);
    }

    #[test]
    fn test_blacklist() {
        assert!(!is_crate_blacklisted("azul_core"));
        assert!(is_crate_blacklisted("azul_dll"));
    }

    #[test]
    fn test_path_blacklist() {
        assert!(is_path_blacklisted("azul_dll::widgets::Button"));
        assert!(!is_path_blacklisted("azul_core::dom::Dom"));
    }

    #[test]
    fn test_search_order() {
        let order = get_search_order();
        assert!(!order.contains(&"azul_dll"));
        assert!(order.contains(&"azul_core"));
    }
}
