//! Nice Rust API generation block
//!
//! Generates the wrapper API that provides ergonomic Rust types
//! by wrapping the FFI types from the dll module.

use super::config::CodegenConfig;
use crate::api::VersionData;

/// Generate re-exports with friendly names
/// e.g., `pub use super::dll::AzDom as Dom;`
pub fn generate_reexports(
    version_data: &VersionData,
    module_name: &str,
    config: &CodegenConfig,
) -> String {
    let mut output = String::new();
    let indent = config.indent(1);
    let prefix = &config.prefix;
    
    output.push_str(&format!("{}// Re-export types with friendly names\n", indent));
    
    if let Some(module_data) = version_data.api.get(module_name) {
        for (class_name, class_data) in &module_data.classes {
            // Skip primitive types and generic type parameters
            if super::config::is_primitive_type(class_name) 
                || super::config::is_generic_type_param(class_name) 
            {
                continue;
            }
            // Skip types without struct_fields or enum_fields - they aren't generated
            // Also include callback_typedef types
            if class_data.struct_fields.is_none() 
                && class_data.enum_fields.is_none() 
                && class_data.callback_typedef.is_none() 
            {
                continue;
            }
            output.push_str(&format!(
                "{}pub use super::dll::{}{} as {};\n",
                indent, prefix, class_name, class_name
            ));
        }
    }
    
    output
}

/// Generate type aliases for API compatibility
/// Maps AzTypeName -> crate::ffi::dll::AzTypeName for patch files
pub fn generate_type_aliases(
    version_data: &VersionData,
    config: &CodegenConfig,
) -> String {
    let mut output = String::new();
    let indent = config.indent(0);
    let prefix = &config.prefix;
    let dll_module = &config.module_paths.dll_module;
    
    output.push_str(&format!("{}// Type aliases for patch compatibility\n", indent));
    
    for module_data in version_data.api.values() {
        for (class_name, class_data) in &module_data.classes {
            // Skip primitive types and generic type parameters
            if super::config::is_primitive_type(class_name) 
                || super::config::is_generic_type_param(class_name) 
            {
                continue;
            }
            // Include types with struct_fields, enum_fields, or callback_typedef
            if class_data.struct_fields.is_some() 
                || class_data.enum_fields.is_some() 
                || class_data.callback_typedef.is_some()
                || class_data.type_alias.is_some()
            {
                output.push_str(&format!(
                    "{}pub type {}{} = {}::{}{};\n",
                    indent, prefix, class_name, dll_module, prefix, class_name
                ));
            }
        }
    }
    
    output
}
