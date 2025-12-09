//! API layer assembly
//!
//! Combines blocks to generate the nice Rust API wrapper (api.rs).
//! This provides ergonomic Rust types that wrap the FFI types.
//!
//! Note: The API layer generation is a new feature. Currently we just
//! generate the FFI layer via `assembly::ffi`, and the API layer
//! is a thin wrapper that re-exports from `crate::ffi::dll`.

use std::{fs, path::Path};

use crate::api::{ApiData, VersionData};
use crate::codegen::blocks::config::{PRIMITIVE_TYPES, is_generic_type_param};

pub type Result<T> = std::result::Result<T, String>;

/// Generate the nice Rust API layer (api.rs)
pub fn generate_api_layer(api_data: &ApiData, output_path: &Path) -> Result<()> {
    println!("  [API] Generating Rust API layer...");
    
    let version_name = api_data
        .0
        .keys()
        .next()
        .ok_or_else(|| "No version name found".to_string())?;
    let version_data = api_data
        .get_version(version_name)
        .ok_or_else(|| "No API version found".to_string())?;
    let prefix = api_data
        .get_version_prefix(version_name)
        .unwrap_or_else(|| "Az".to_string());
    
    // Create output directory
    fs::create_dir_all(output_path.parent().unwrap())
        .map_err(|e| format!("Failed to create output dir: {}", e))?;
    
    // Generate the content
    let content = generate_api_content(version_data, &prefix)?;
    
    println!("  [SAVE] Writing api.rs ({} bytes)...", content.len());
    fs::write(output_path, content)
        .map_err(|e| format!("Failed to write api.rs: {}", e))?;
    
    println!("[OK] Generated API layer at: {}", output_path.display());
    
    Ok(())
}

/// Generate the API layer content
fn generate_api_content(
    version_data: &VersionData,
    prefix: &str,
) -> Result<String> {
    let mut output = String::new();
    
    // Header
    output.push_str("// Nice Rust API wrapper for azul-dll\n");
    output.push_str("//\n");
    output.push_str("// This module provides ergonomic Rust types that wrap the FFI layer.\n");
    output.push_str("// All types delegate to `crate::ffi::dll::*` for actual implementation.\n\n");
    
    output.push_str("#[allow(dead_code, unused_imports)]\n\n");
    output.push_str("use core::ffi::c_void;\n\n");
    
    // Re-export all types from the FFI dll module with unprefixed names
    output.push_str("// ===== Type Re-exports =====\n");
    output.push_str("// Re-export FFI types with friendly (unprefixed) names\n\n");
    
    let mut all_types: Vec<(String, String)> = Vec::new();
    
    for module_data in version_data.api.values() {
        for (class_name, class_data) in &module_data.classes {
            if PRIMITIVE_TYPES.contains(&class_name.as_str()) || is_generic_type_param(class_name) {
                continue;
            }
            
            // Skip generic types - they can't have simple type aliases
            if class_data.generic_params.as_ref().map(|p| !p.is_empty()).unwrap_or(false) {
                continue;
            }
            
            // Generate alias for types that have definitions
            if class_data.struct_fields.is_some() 
                || class_data.enum_fields.is_some() 
                || class_data.callback_typedef.is_some()
                || class_data.type_alias.is_some()
            {
                let prefixed_name = format!("{}{}", prefix, class_name);
                all_types.push((class_name.clone(), prefixed_name));
            }
        }
    }
    
    // Sort and deduplicate
    all_types.sort_by(|a, b| a.0.cmp(&b.0));
    all_types.dedup_by(|a, b| a.0 == b.0);
    
    for (unprefixed, prefixed) in &all_types {
        // pub use crate::ffi::dll::AzDom as Dom;
        output.push_str(&format!(
            "pub use crate::ffi::dll::{} as {};\n",
            prefixed, unprefixed
        ));
    }
    
    // Prelude module
    output.push_str("\n// ===== Prelude =====\n");
    output.push_str("pub mod prelude {\n");
    output.push_str("    //! Re-exports commonly used types for convenient imports.\n");
    output.push_str("    //!\n");
    output.push_str("    //! ```rust,ignore\n");
    output.push_str("    //! use azul_dll::prelude::*;\n");
    output.push_str("    //! ```\n\n");
    
    for (unprefixed, _prefixed) in &all_types {
        output.push_str(&format!("    pub use super::{};\n", unprefixed));
    }
    output.push_str("}\n");
    
    Ok(output)
}
