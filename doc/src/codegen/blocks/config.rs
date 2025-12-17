//! Central configuration for code generation
//!
//! This module defines the shared configuration types used by all code generation
//! blocks to ensure consistent output.

use std::collections::{BTreeMap, BTreeSet};

/// Configuration for all code generation operations
#[derive(Debug, Clone)]
pub struct CodegenConfig {
    /// Type prefix for generated types (e.g., "Az", "Az1")
    pub prefix: String,
    
    /// Indentation style (number of spaces per level)
    pub indent_spaces: usize,
    
    /// Whether this is for the FFI layer (uses raw types) or API layer (uses wrappers)
    pub is_ffi: bool,
    
    /// Whether to generate function bodies (true) or stubs (false)
    pub generate_fn_bodies: bool,
    
    /// Whether to generate serde derives when available
    pub include_serde: bool,
    
    /// Target module paths for type resolution
    pub module_paths: ModulePaths,
}

impl Default for CodegenConfig {
    fn default() -> Self {
        Self {
            prefix: "Az".to_string(),
            indent_spaces: 4,
            is_ffi: true,
            generate_fn_bodies: true,
            include_serde: false,
            module_paths: ModulePaths::ffi(),
        }
    }
}

impl CodegenConfig {
    /// Create a config for FFI layer generation (dll_api.rs)
    pub fn for_ffi() -> Self {
        Self {
            prefix: "Az".to_string(),
            indent_spaces: 4,
            is_ffi: true,
            generate_fn_bodies: true,
            include_serde: false,
            module_paths: ModulePaths::ffi(),
        }
    }
    
    /// Create a config for API layer generation (api.rs)
    pub fn for_api() -> Self {
        Self {
            prefix: "".to_string(), // No prefix for nice API
            indent_spaces: 4,
            is_ffi: false,
            generate_fn_bodies: false, // API layer delegates to FFI
            include_serde: false,
            module_paths: ModulePaths::api(),
        }
    }
    
    /// Create a config for memtest generation
    pub fn for_memtest() -> Self {
        Self {
            prefix: "Az".to_string(),
            indent_spaces: 4,
            is_ffi: true,
            generate_fn_bodies: false, // Memtest uses stubs
            include_serde: false,
            module_paths: ModulePaths::memtest(),
        }
    }
    
    /// Get indent string for a given nesting level
    pub fn indent(&self, level: usize) -> String {
        " ".repeat(self.indent_spaces * level)
    }
}

/// Module path configuration for different generation targets
#[derive(Debug, Clone)]
pub struct ModulePaths {
    /// Path to the FFI dll module (e.g., "crate::ffi::dll" or "super::dll")
    pub dll_module: String,
    
    /// Path to the vec module
    pub vec_module: String,
    
    /// Path to the option module
    pub option_module: String,
    
    /// Path to the str module
    pub str_module: String,
    
    /// Path to the dom module
    pub dom_module: String,
    
    /// Path to the callbacks module
    pub callbacks_module: String,
    
    /// Path to the gl module
    pub gl_module: String,
    
    /// Path to the css module
    pub css_module: String,
    
    /// Path to the window module
    pub window_module: String,
    
    /// Prefix for types from external crates (e.g., "azul_core::" -> "crate::ffi::dll::")
    pub external_crate_replacement: BTreeMap<String, String>,
}

impl ModulePaths {
    /// Paths for FFI generation (used in dll_api.rs, included in azul-dll crate)
    pub fn ffi() -> Self {
        let mut external_crate_replacement = BTreeMap::new();
        external_crate_replacement.insert("azul_dll::".to_string(), "crate::ffi::dll::".to_string());
        external_crate_replacement.insert("azul_core::".to_string(), "crate::ffi::dll::".to_string());
        external_crate_replacement.insert("azul_css::".to_string(), "crate::ffi::dll::".to_string());
        external_crate_replacement.insert("azul_layout::".to_string(), "crate::ffi::dll::".to_string());
        
        Self {
            dll_module: "crate::ffi::dll".to_string(),
            vec_module: "crate::ffi::vec".to_string(),
            option_module: "crate::ffi::option".to_string(),
            str_module: "crate::ffi::str".to_string(),
            dom_module: "crate::ffi::dom".to_string(),
            callbacks_module: "crate::ffi::callbacks".to_string(),
            gl_module: "crate::ffi::gl".to_string(),
            css_module: "crate::ffi::css".to_string(),
            window_module: "crate::ffi::window".to_string(),
            external_crate_replacement,
        }
    }
    
    /// Paths for API generation (uses FFI types via crate::ffi::*)
    pub fn api() -> Self {
        let mut external_crate_replacement = BTreeMap::new();
        external_crate_replacement.insert("azul_dll::".to_string(), "crate::ffi::dll::".to_string());
        external_crate_replacement.insert("azul_core::".to_string(), "crate::ffi::dll::".to_string());
        external_crate_replacement.insert("azul_css::".to_string(), "crate::ffi::dll::".to_string());
        external_crate_replacement.insert("azul_layout::".to_string(), "crate::ffi::dll::".to_string());
        
        Self {
            dll_module: "crate::ffi::dll".to_string(),
            vec_module: "crate::ffi::vec".to_string(),
            option_module: "crate::ffi::option".to_string(),
            str_module: "crate::ffi::str".to_string(),
            dom_module: "crate::ffi::dom".to_string(),
            callbacks_module: "crate::ffi::callbacks".to_string(),
            gl_module: "crate::ffi::gl".to_string(),
            css_module: "crate::ffi::css".to_string(),
            window_module: "crate::ffi::window".to_string(),
            external_crate_replacement,
        }
    }
    
    /// Paths for memtest generation (standalone test crate)
    pub fn memtest() -> Self {
        let mut external_crate_replacement = BTreeMap::new();
        // In memtest, external crate paths are kept as-is for size/align comparison
        
        Self {
            dll_module: "crate::generated::dll".to_string(),
            vec_module: "crate::generated::vec".to_string(),
            option_module: "crate::generated::option".to_string(),
            str_module: "crate::generated::str".to_string(),
            dom_module: "crate::generated::dom".to_string(),
            callbacks_module: "crate::generated::callbacks".to_string(),
            gl_module: "crate::generated::gl".to_string(),
            css_module: "crate::generated::css".to_string(),
            window_module: "crate::generated::window".to_string(),
            external_crate_replacement,
        }
    }
    
    /// Replace external crate paths with the configured replacement
    pub fn replace_external_paths(&self, code: &str) -> String {
        let mut result = code.to_string();
        for (from, to) in &self.external_crate_replacement {
            result = result.replace(from, to);
        }
        result
    }
}

/// Collected type metadata for code generation
#[derive(Debug)]
pub struct TypeMetadata {
    /// Set of all type names (unprefixed) that exist in the API
    pub type_names: BTreeSet<String>,
    
    /// Map from prefixed type name to external path
    pub type_to_external: BTreeMap<String, String>,
    
    /// Map from prefixed type name to module name
    pub type_to_module: BTreeMap<String, String>,
}

impl TypeMetadata {
    pub fn new() -> Self {
        Self {
            type_names: BTreeSet::new(),
            type_to_external: BTreeMap::new(),
            type_to_module: BTreeMap::new(),
        }
    }
}

/// Primitive types that should never get a prefix
pub const PRIMITIVE_TYPES: &[&str] = &[
    "bool", "f32", "f64", "fn", "i128", "i16", "i32", "i64", "i8", "isize",
    "slice", "u128", "u16", "u32", "u64", "u8", "usize", "c_void",
    "str", "char", "c_char", "c_schar", "c_uchar",
];

/// Check if a type name is a primitive type
pub fn is_primitive_type(type_name: &str) -> bool {
    PRIMITIVE_TYPES.contains(&type_name)
}

/// Check if a type name is a generic type parameter (single uppercase letter)
pub fn is_generic_type_param(type_name: &str) -> bool {
    type_name.len() == 1 && type_name.chars().next().map(|c| c.is_ascii_uppercase()).unwrap_or(false)
}
