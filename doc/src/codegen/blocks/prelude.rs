//! Prelude generation block
//!
//! Generates the prelude module that re-exports commonly used types
//! for convenient importing via `use azul_dll::prelude::*`

use super::config::CodegenConfig;
use crate::api::VersionData;

/// Generate prelude re-exports for all public types
pub fn generate_prelude(
    version_data: &VersionData,
    config: &CodegenConfig,
) -> String {
    let mut output = String::new();
    let indent = config.indent(0);
    
    output.push_str(&format!("{}//! Common types re-exported for convenience\n", indent));
    output.push_str(&format!("{}//! \n", indent));
    output.push_str(&format!("{}//! ```rust\n", indent));
    output.push_str(&format!("{}//! use azul_dll::prelude::*;\n", indent));
    output.push_str(&format!("{}//! ```\n\n", indent));
    
    // Re-export all non-primitive, non-generic types
    let mut exported_types: Vec<String> = Vec::new();
    
    for module_data in version_data.api.values() {
        for (class_name, class_data) in &module_data.classes {
            // Skip primitive types and generic type parameters
            if super::config::is_primitive_type(class_name) 
                || super::config::is_generic_type_param(class_name) 
            {
                continue;
            }
            // Include types with actual definitions
            if class_data.struct_fields.is_some() 
                || class_data.enum_fields.is_some() 
                || class_data.callback_typedef.is_some()
                || class_data.type_alias.is_some()
            {
                exported_types.push(class_name.clone());
            }
        }
    }
    
    // Sort for deterministic output
    exported_types.sort();
    exported_types.dedup();
    
    // Group re-exports
    output.push_str(&format!("{}pub use crate::api::{{\n", indent));
    
    for (i, type_name) in exported_types.iter().enumerate() {
        if i > 0 && i % 8 == 0 {
            output.push_str("\n");
        }
        if i == exported_types.len() - 1 {
            output.push_str(&format!("    {},\n", type_name));
        } else {
            output.push_str(&format!("    {}, ", type_name));
        }
    }
    
    output.push_str(&format!("{}}};\n", indent));
    
    output
}
