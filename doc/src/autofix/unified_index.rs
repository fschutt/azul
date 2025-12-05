//! Unified Type Index - combines TypeIndex and WorkspaceIndex
//!
//! This module provides a unified interface for type lookups that:
//! 1. First checks TypeIndex (has macro-generated types)
//! 2. Falls back to WorkspaceIndex for types not found in TypeIndex
//!
//! This ensures macro-generated types (impl_vec!, impl_option!, etc.) are
//! properly expanded with their fields, while still supporting all other types.

use std::path::Path;

use crate::autofix::type_index::TypeIndex;
use crate::patch::index::{ParsedTypeInfo, WorkspaceIndex};

/// Trait for type lookups - implemented by both TypeIndex and WorkspaceIndex
pub trait TypeLookup {
    /// Find a type by name
    fn find_type(&self, type_name: &str) -> Option<Vec<ParsedTypeInfo>>;
    
    /// Find a type by full path
    fn find_type_by_path(&self, type_path: &str) -> Option<ParsedTypeInfo>;
    
    /// Find a type by string search (for macro-defined types)
    fn find_type_by_string_search(&self, type_name: &str) -> Option<ParsedTypeInfo>;
}

// Implement TypeLookup for TypeIndex
impl TypeLookup for TypeIndex {
    fn find_type(&self, type_name: &str) -> Option<Vec<ParsedTypeInfo>> {
        self.find_type(type_name)
    }
    
    fn find_type_by_path(&self, type_path: &str) -> Option<ParsedTypeInfo> {
        self.find_type_by_path(type_path)
    }
    
    fn find_type_by_string_search(&self, type_name: &str) -> Option<ParsedTypeInfo> {
        self.find_type_by_string_search(type_name)
    }
}

// Implement TypeLookup for WorkspaceIndex
impl TypeLookup for WorkspaceIndex {
    fn find_type(&self, type_name: &str) -> Option<Vec<ParsedTypeInfo>> {
        self.find_type(type_name).map(|v| v.to_vec())
    }
    
    fn find_type_by_path(&self, type_path: &str) -> Option<ParsedTypeInfo> {
        self.find_type_by_path(type_path).cloned()
    }
    
    fn find_type_by_string_search(&self, type_name: &str) -> Option<ParsedTypeInfo> {
        self.find_type_by_string_search(type_name)
    }
}

/// Unified type index that combines TypeIndex and WorkspaceIndex
pub struct UnifiedTypeIndex {
    /// TypeIndex - has MacroGenerated type expansion
    type_index: TypeIndex,
    /// WorkspaceIndex - has more detailed parsing for regular types
    workspace_index: WorkspaceIndex,
}

impl UnifiedTypeIndex {
    /// Build a unified index from the workspace root
    pub fn build(workspace_root: &Path, verbose: bool) -> anyhow::Result<Self> {
        let type_index = TypeIndex::build(workspace_root, verbose)?;
        let workspace_index = WorkspaceIndex::build(workspace_root)?;
        
        Ok(Self {
            type_index,
            workspace_index,
        })
    }
    
    /// Get access to the underlying TypeIndex
    pub fn type_index(&self) -> &TypeIndex {
        &self.type_index
    }
    
    /// Get access to the underlying WorkspaceIndex
    pub fn workspace_index(&self) -> &WorkspaceIndex {
        &self.workspace_index
    }
}

// Implement TypeLookup for UnifiedTypeIndex - prefers TypeIndex over WorkspaceIndex
impl TypeLookup for UnifiedTypeIndex {
    fn find_type(&self, type_name: &str) -> Option<Vec<ParsedTypeInfo>> {
        // First try TypeIndex - it has MacroGenerated types expanded
        if let Some(parsed) = self.type_index.find_type(type_name) {
            return Some(parsed);
        }
        
        // Fall back to WorkspaceIndex
        self.workspace_index.find_type(type_name).map(|v| v.to_vec())
    }
    
    fn find_type_by_path(&self, type_path: &str) -> Option<ParsedTypeInfo> {
        // First try TypeIndex
        if let Some(parsed) = self.type_index.find_type_by_path(type_path) {
            return Some(parsed);
        }
        
        // Fall back to WorkspaceIndex
        self.workspace_index.find_type_by_path(type_path).cloned()
    }
    
    fn find_type_by_string_search(&self, type_name: &str) -> Option<ParsedTypeInfo> {
        // First try TypeIndex
        if let Some(parsed) = self.type_index.find_type_by_string_search(type_name) {
            return Some(parsed);
        }
        
        // Fall back to WorkspaceIndex
        self.workspace_index.find_type_by_string_search(type_name)
    }
}
