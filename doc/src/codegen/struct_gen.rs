//! Struct, Enum, and Typedef generation
//!
//! This module generates Rust struct/enum definitions from api.json data.
//! It's a direct port of the `generate_structs` function from oldbuild.py.

use std::collections::HashMap;

use anyhow::Result;
use indexmap::IndexMap;

use crate::{
    api::{ClassData, EnumVariantData, FieldData, VersionData},
    utils::analyze::{analyze_type, get_class, is_primitive_arg, search_for_class_by_class_name},
};

/// Configuration for struct generation
#[derive(Debug, Clone)]
pub struct GenerateConfig {
    /// Type prefix (e.g., "Az", "Az1", "Az2" based on version)
    pub prefix: String,
    /// Indentation level (number of spaces)
    pub indent: usize,
    /// Whether to make pointer fields pub instead of pub
    pub private_pointers: bool,
    /// Whether to skip all derives (for Python bindings)
    pub no_derive: bool,
    /// Suffix to add to enum wrapper types
    pub wrapper_postfix: String,
}

impl Default for GenerateConfig {
    fn default() -> Self {
        Self {
            prefix: "Az".to_string(),
            indent: 4,
            private_pointers: true,
            no_derive: false,
            wrapper_postfix: String::new(),
        }
    }
}

/// Struct metadata extracted from ClassData
#[derive(Debug, Clone)]
pub struct StructMetadata {
    pub name: String,
    pub doc: Option<String>,
    pub external: Option<String>,
    pub derive: Vec<String>,
    /// True if derive was explicitly set (even if empty), false if defaulted
    /// When true and derive is empty, no auto-derives will be generated
    pub has_explicit_derive: bool,
    pub is_callback_typedef: bool,
    pub can_be_copied: bool,
    pub can_be_serde_serialized: bool,
    pub can_be_serde_deserialized: bool,
    pub implements_default: bool,
    pub implements_eq: bool,
    pub implements_ord: bool,
    pub implements_hash: bool,
    pub has_custom_destructor: bool,
    pub can_be_cloned: bool,
    pub is_boxed_object: bool,
    pub treat_external_as_ptr: bool,
    pub serde_extra: Option<String>,
    pub repr: Option<String>,
    pub struct_fields: Option<Vec<IndexMap<String, FieldData>>>,
    pub enum_fields: Option<Vec<IndexMap<String, EnumVariantData>>>,
    pub callback_typedef: Option<crate::api::CallbackDefinition>,
    pub type_alias: Option<crate::api::TypeAliasInfo>,
    pub generic_params: Option<Vec<String>>,
    /// Traits with manual implementations (e.g., ["Clone", "Drop"])
    pub custom_impls: Vec<String>,
    /// For VecRef types: the element type (e.g., "u8" for U8VecRef)
    pub vec_ref_element_type: Option<String>,
    /// Whether this is a mutable VecRef (VecRefMut)
    pub vec_ref_is_mut: bool,
}

impl StructMetadata {
    /// Extract metadata from ClassData
    pub fn from_class_data(name: String, class_data: &ClassData) -> Self {
        let has_explicit_derive = class_data.derive.is_some();
        let derive = class_data.derive.clone().unwrap_or_default();

        let is_callback_typedef = class_data.callback_typedef.is_some();
        let can_be_copied = derive.contains(&"Copy".to_string());
        let can_be_serde_serialized = derive.contains(&"Serialize".to_string());
        let can_be_serde_deserialized = derive.contains(&"Deserialize".to_string());
        let implements_default = derive.contains(&"Default".to_string());
        let implements_eq = derive.contains(&"Eq".to_string());
        let implements_ord = derive.contains(&"Ord".to_string());
        let implements_hash = derive.contains(&"Hash".to_string());

        let has_custom_destructor = class_data.has_custom_drop();
        let can_be_cloned = class_data.can_derive_clone();
        let is_boxed_object = class_data.is_boxed_object;
        let treat_external_as_ptr = class_data.external.is_some() && is_boxed_object;
        let custom_impls = class_data.custom_impls.clone().unwrap_or_default();

        // DEBUG: Print custom_impls for RefAny specifically
        if name == "RefAny" {
            eprintln!("[DEBUG from_class_data] RefAny:");
            eprintln!("  class_data.custom_impls: {:?}", class_data.custom_impls);
            eprintln!("  custom_impls: {:?}", custom_impls);
            eprintln!("  has_custom_clone: {}", has_custom_destructor);
            eprintln!("  can_be_cloned: {}", can_be_cloned);
        }
        
        // DEBUG: Print custom_impls for types that have them
        if !custom_impls.is_empty() {
            eprintln!("[DEBUG] Type '{}' has custom_impls: {:?}", name, custom_impls);
        }

        Self {
            name,
            doc: class_data.doc.clone(),
            external: class_data.external.clone(),
            derive,
            has_explicit_derive,
            is_callback_typedef,
            can_be_copied,
            can_be_serde_serialized,
            can_be_serde_deserialized,
            implements_default,
            implements_eq,
            implements_ord,
            implements_hash,
            has_custom_destructor,
            can_be_cloned,
            is_boxed_object,
            treat_external_as_ptr,
            serde_extra: class_data.serde.clone(),
            repr: None, // Set later if needed
            struct_fields: class_data.struct_fields.clone(),
            enum_fields: class_data.enum_fields.clone(),
            callback_typedef: class_data.callback_typedef.clone(),
            type_alias: class_data.type_alias.clone(),
            generic_params: class_data.generic_params.clone(),
            custom_impls,
            vec_ref_element_type: class_data.vec_ref_element_type.clone(),
            vec_ref_is_mut: class_data.vec_ref_is_mut,
        }
    }
}

/// Generate Rust struct/enum definitions from a map of structs
pub fn generate_structs(
    version_data: &VersionData,
    structs_map: &HashMap<String, StructMetadata>,
    config: &GenerateConfig,
) -> Result<String> {
    let indent_str = " ".repeat(config.indent);
    let mut code = String::new();

    // First pass: collect all callback_typedef type names (without prefix)
    // These are function pointer types like CallbackType, LayoutCallbackType, etc.
    let callback_typedef_types: std::collections::HashSet<String> = structs_map
        .iter()
        .filter(|(_, meta)| meta.is_callback_typedef)
        .map(|(name, _)| {
            // Remove prefix to get base type name
            name.strip_prefix(&config.prefix).unwrap_or(name).to_string()
        })
        .collect();

    // Sort structs to ensure fundamental types come first
    // This is necessary because derive(Clone) on a struct requires its fields to be Clone
    // Types like AzU8Vec, AzString must be defined before structs that use them
    let mut sorted_structs: Vec<_> = structs_map.iter().collect();
    sorted_structs.sort_by(|(name_a, _), (name_b, _)| {
        // Define priority: lower number = earlier in output
        fn priority(name: &str) -> u32 {
            // Most fundamental types first (used by many other types)
            if name.ends_with("Vec") && !name.contains("VecDestructor") && !name.contains("VecRef") {
                return 0; // U8Vec, StringVec, etc.
            }
            if name.ends_with("String") {
                return 1; // AzString
            }
            if name.ends_with("VecRef") {
                return 2; // VecRef types
            }
            if name.ends_with("VecDestructor") {
                return 3; // VecDestructor types
            }
            if name.contains("Option") {
                return 4; // Option types
            }
            // Everything else at same priority, sort alphabetically for determinism
            10
        }
        let priority_a = priority(name_a);
        let priority_b = priority(name_b);
        if priority_a != priority_b {
            priority_a.cmp(&priority_b)
        } else {
            name_a.cmp(name_b)
        }
    });

    for (struct_name, struct_meta) in sorted_structs {
        code.push_str(&generate_single_type(
            version_data,
            struct_name,
            struct_meta,
            config,
            &indent_str,
            &callback_typedef_types,
        )?);
    }

    Ok(code)
}

fn generate_single_type(
    version_data: &VersionData,
    struct_name: &str,
    struct_meta: &StructMetadata,
    config: &GenerateConfig,
    indent_str: &str,
    callback_typedef_types: &std::collections::HashSet<String>,
) -> Result<String> {
    let mut code = String::new();

    // Add documentation
    if let Some(doc) = &struct_meta.doc {
        code.push_str(&format!("{}/// {}\n", indent_str, doc));
    } else {
        code.push_str(&format!("{}/// `{}` struct\n", indent_str, struct_name));
    }

    // Handle type aliases (generic type instantiations)
    if let Some(type_alias_info) = &struct_meta.type_alias {
        // Check if target is a primitive type or function pointer (shouldn't get prefix)
        let is_primitive = matches!(
            type_alias_info.target.as_str(),
            "u8" | "u16"
                | "u32"
                | "u64"
                | "u128"
                | "usize"
                | "i8"
                | "i16"
                | "i32"
                | "i64"
                | "i128"
                | "isize"
                | "f32"
                | "f64"
                | "bool"
                | "char"
                | "str"
                | "c_void"
        );

        // Check if target is a function pointer (starts with "extern")
        let is_function_ptr = type_alias_info.target.starts_with("extern");
        
        // Check if target is a pointer type (*const T or *mut T)
        let is_pointer_type = type_alias_info.target.starts_with("*const ") 
            || type_alias_info.target.starts_with("*mut ");

        let target_name = if is_primitive {
            type_alias_info.target.clone()
        } else if is_function_ptr {
            // For extern fn types, we need to prefix all types within the function signature
            prefix_types_in_extern_fn_string(version_data, &type_alias_info.target, &config.prefix)
        } else if is_pointer_type {
            // For pointer types like "*mut c_void", don't add prefix
            type_alias_info.target.clone()
        } else {
            format!("{}{}", config.prefix, type_alias_info.target)
        };

        if type_alias_info.generic_args.is_empty() {
            // Simple type alias without generics: pub type AzGLuint = u32;
            code.push_str(&format!(
                "{}pub type {} = {};\n\n",
                indent_str, struct_name, target_name
            ));
        } else {
            // Generic type alias: pub type AzFooValue = AzCssPropertyValue<AzFoo>;
            let args_with_prefix: Vec<String> = type_alias_info
                .generic_args
                .iter()
                .map(|arg| {
                    // Don't prefix primitive types
                    if is_primitive_arg(arg) {
                        arg.clone()
                    } else {
                        format!("{}{}", config.prefix, arg)
                    }
                })
                .collect();

            code.push_str(&format!(
                "{}pub type {} = {}<{}>;\n\n",
                indent_str,
                struct_name,
                target_name,
                args_with_prefix.join(", ")
            ));
        }
        return Ok(code);
    }

    // Handle callback typedefs
    if struct_meta.is_callback_typedef {
        if let Some(callback_typedef) = &struct_meta.callback_typedef {
            let fn_ptr =
                generate_rust_callback_fn_type(version_data, callback_typedef, &config.prefix)?;
            code.push_str(&format!(
                "{}pub type {} = {};\n\n",
                indent_str, struct_name, fn_ptr
            ));
            return Ok(code);
        }
    }

    // Handle structs
    if let Some(struct_fields) = &struct_meta.struct_fields {
        code.push_str(&generate_struct_definition(
            version_data,
            struct_name,
            struct_meta,
            struct_fields,
            config,
            indent_str,
            callback_typedef_types,
        )?);

        // Generate callback trait implementations if this struct wraps a callback_typedef type
        // A callback wrapper struct has exactly one field whose type is a callback_typedef
        if let Some(field_name) = get_callback_wrapper_field(struct_fields, callback_typedef_types) {
            code.push_str(&generate_callback_trait_impls(
                struct_name,
                &field_name,
                config,
                indent_str,
            )?);
        }
    }
    // Handle enums
    else if let Some(enum_fields) = &struct_meta.enum_fields {
        code.push_str(&generate_enum_definition(
            version_data,
            struct_name,
            struct_meta,
            enum_fields,
            config,
            indent_str,
        )?);
    }

    Ok(code)
}

/// Check if a struct is a callback wrapper struct
/// A callback wrapper has exactly one field whose type is a callback_typedef type
/// Examples: Callback (has cb: CallbackType), IFrameCallback (has cb: IFrameCallbackType)
/// Returns the field name if it's a callback wrapper, None otherwise
fn get_callback_wrapper_field(
    struct_fields: &[IndexMap<String, FieldData>],
    callback_typedef_types: &std::collections::HashSet<String>,
) -> Option<String> {
    // Count total fields
    let total_fields: usize = struct_fields.iter().map(|m| m.len()).sum();
    
    // Must have exactly one field
    if total_fields != 1 {
        return None;
    }
    
    // Check if that single field's type is a callback_typedef
    for field_map in struct_fields {
        for (field_name, field_data) in field_map {
            let field_type = &field_data.r#type;
            // Check if this type is in our callback_typedef set
            if callback_typedef_types.contains(field_type) {
                return Some(field_name.clone());
            }
        }
    }
    None
}

/// Generate trait implementations for callback structs
/// This replicates the behavior of the `impl_callback!` macro from core/src/macros.rs
/// The `field_name` is the name of the single field containing the function pointer
fn generate_callback_trait_impls(
    struct_name: &str,
    field_name: &str,
    config: &GenerateConfig,
    indent_str: &str,
) -> Result<String> {
    let mut code = String::new();

    // Debug implementation - shows type name and pointer address
    code.push_str(&format!(
        "\n{}impl ::core::fmt::Debug for {} {{\n",
        indent_str, struct_name
    ));
    code.push_str(&format!(
        "{}    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {{\n",
        indent_str
    ));
    code.push_str(&format!(
        "{}        write!(f, \"{} @ 0x{{:x}}\", self.{} as usize)\n",
        indent_str, struct_name, field_name
    ));
    code.push_str(&format!("{}    }}\n", indent_str));
    code.push_str(&format!("{}}}\n", indent_str));

    // Clone implementation
    code.push_str(&format!(
        "\n{}impl Clone for {} {{\n",
        indent_str, struct_name
    ));
    code.push_str(&format!("{}    fn clone(&self) -> Self {{\n", indent_str));
    code.push_str(&format!(
        "{}        {} {{ {}: self.{}.clone() }}\n",
        indent_str, struct_name, field_name, field_name
    ));
    code.push_str(&format!("{}    }}\n", indent_str));
    code.push_str(&format!("{}}}\n", indent_str));

    // Hash implementation
    code.push_str(&format!(
        "\n{}impl ::core::hash::Hash for {} {{\n",
        indent_str, struct_name
    ));
    code.push_str(&format!(
        "{}    fn hash<H>(&self, state: &mut H)\n",
        indent_str
    ));
    code.push_str(&format!("{}    where\n", indent_str));
    code.push_str(&format!("{}        H: ::core::hash::Hasher,\n", indent_str));
    code.push_str(&format!("{}    {{\n", indent_str));
    code.push_str(&format!(
        "{}        state.write_usize(self.{} as usize);\n",
        indent_str, field_name
    ));
    code.push_str(&format!("{}    }}\n", indent_str));
    code.push_str(&format!("{}}}\n", indent_str));

    // PartialEq implementation
    code.push_str(&format!(
        "\n{}impl PartialEq for {} {{\n",
        indent_str, struct_name
    ));
    code.push_str(&format!(
        "{}    fn eq(&self, rhs: &Self) -> bool {{\n",
        indent_str
    ));
    code.push_str(&format!(
        "{}        self.{} as usize == rhs.{} as usize\n",
        indent_str, field_name, field_name
    ));
    code.push_str(&format!("{}    }}\n", indent_str));
    code.push_str(&format!("{}}}\n", indent_str));

    // PartialOrd implementation
    code.push_str(&format!(
        "\n{}impl PartialOrd for {} {{\n",
        indent_str, struct_name
    ));
    code.push_str(&format!(
        "{}    fn partial_cmp(&self, other: &Self) -> Option<::core::cmp::Ordering> {{\n",
        indent_str
    ));
    code.push_str(&format!(
        "{}        Some((self.{} as usize).cmp(&(other.{} as usize)))\n",
        indent_str, field_name, field_name
    ));
    code.push_str(&format!("{}    }}\n", indent_str));
    code.push_str(&format!("{}}}\n", indent_str));

    // Ord implementation
    code.push_str(&format!(
        "\n{}impl Ord for {} {{\n",
        indent_str, struct_name
    ));
    code.push_str(&format!(
        "{}    fn cmp(&self, other: &Self) -> ::core::cmp::Ordering {{\n",
        indent_str
    ));
    code.push_str(&format!(
        "{}        (self.{} as usize).cmp(&(other.{} as usize))\n",
        indent_str, field_name, field_name
    ));
    code.push_str(&format!("{}    }}\n", indent_str));
    code.push_str(&format!("{}}}\n", indent_str));

    // Eq implementation (marker trait)
    code.push_str(&format!(
        "\n{}impl Eq for {} {{}}\n",
        indent_str, struct_name
    ));

    // Copy implementation (marker trait) - function pointers are Copy
    code.push_str(&format!(
        "\n{}impl Copy for {} {{}}\n",
        indent_str, struct_name
    ));

    Ok(code)
}

fn generate_struct_definition(
    version_data: &VersionData,
    struct_name: &str,
    struct_meta: &StructMetadata,
    struct_fields: &[IndexMap<String, FieldData>],
    config: &GenerateConfig,
    indent_str: &str,
    callback_typedef_types: &std::collections::HashSet<String>,
) -> Result<String> {
    let mut code = String::new();

    // SIMPLIFIED: Use derives directly from api.json (struct_meta.derive)
    // All derive information is now explicit in api.json - no auto-computation
    // BUT: Serialize/Deserialize need special handling - they go in cfg_attr
    let mut derives: Vec<&str> = Vec::new();
    let mut has_serialize = false;
    let mut has_deserialize = false;
    
    if !config.no_derive {
        // Add derives from api.json, but filter out Serialize/Deserialize
        for d in &struct_meta.derive {
            match d.as_str() {
                "Serialize" => has_serialize = true,
                "Deserialize" => has_deserialize = true,
                other => derives.push(other),
            }
        }
    }
    
    // For callback wrapper structs, don't derive any traits because we generate custom implementations
    let is_callback_wrapper = get_callback_wrapper_field(struct_fields, callback_typedef_types).is_some();
    if is_callback_wrapper {
        derives.clear();
        has_serialize = false;
        has_deserialize = false;
    }
    
    // For VecRef types, don't derive traits because we generate custom implementations using as_slice()
    if struct_meta.vec_ref_element_type.is_some() {
        derives.clear();
        has_serialize = false;
        has_deserialize = false;
    }

    // Generate derive attributes
    let derive_str = if derives.is_empty() {
        String::new()
    } else {
        format!("{}#[derive({})]\n", indent_str, derives.join(", "))
    };

    // Serde derives - use Serialize/Deserialize from api.json derives array
    let opt_derive_serde = if config.no_derive {
        String::new()
    } else if has_serialize && has_deserialize {
        format!(
            "{}#[cfg_attr(feature = \"serde-support\", derive(Serialize, Deserialize))]\n",
            indent_str
        )
    } else if has_serialize {
        format!(
            "{}#[cfg_attr(feature = \"serde-support\", derive(Serialize))]\n",
            indent_str
        )
    } else if has_deserialize {
        format!(
            "{}#[cfg_attr(feature = \"serde-support\", derive(Deserialize))]\n",
            indent_str
        )
    } else {
        String::new()
    };

    let opt_derive_serde_extra = if !config.no_derive && !opt_derive_serde.is_empty() {
        if let Some(serde_extra) = &struct_meta.serde_extra {
            format!(
                "{}#[cfg_attr(feature = \"serde-support\", serde({}))]\n",
                indent_str, serde_extra
            )
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    // repr attribute
    let repr = struct_meta
        .repr
        .as_ref()
        .map(|r| format!("{}#[repr({})]\n", indent_str, r))
        .unwrap_or_else(|| format!("{}#[repr(C)]\n", indent_str));

    // Write all attributes
    code.push_str(&repr);
    code.push_str(&derive_str);
    code.push_str(&opt_derive_serde);
    code.push_str(&opt_derive_serde_extra);

    // Add generic parameters if present
    let generic_params_str = if let Some(params) = &struct_meta.generic_params {
        if params.is_empty() {
            String::new()
        } else {
            format!("<{}>", params.join(", "))
        }
    } else {
        String::new()
    };

    code.push_str(&format!(
        "{}pub struct {}{} {{\n",
        indent_str, struct_name, generic_params_str
    ));

    // Generate fields
    for field_map in struct_fields {
        for (field_name, field_data) in field_map {
            let field_type = &field_data.r#type;

            // Check if this is a generic type parameter (like T)
            let is_generic_param = struct_meta
                .generic_params
                .as_ref()
                .map(|params| params.contains(&field_type.to_string()))
                .unwrap_or(false);

            if is_generic_param {
                // Generic type parameter - use as-is
                let visibility = if field_name == "ptr" && config.private_pointers {
                    "pub(crate)"
                } else {
                    "pub"
                };
                code.push_str(&format!(
                    "{}    {} {}: {},\n",
                    indent_str, visibility, field_name, field_type
                ));
            } else {
                let (prefix, base_type, suffix) = analyze_type(field_type);

                if is_primitive_arg(&base_type) {
                    // Primitive type - use as-is
                    let visibility = if field_name == "ptr" && config.private_pointers {
                        "pub(crate)"
                    } else {
                        "pub"
                    };
                    code.push_str(&format!(
                        "{}    {} {}: {},\n",
                        indent_str, visibility, field_name, field_type
                    ));
                } else {
                    // Complex type - need to resolve and add prefix
                    
                    // Look up the type in api.json
                    let resolved_class_name = if let Some((_, class_name)) = 
                        search_for_class_by_class_name(version_data, &base_type) 
                    {
                        Some(class_name.to_string())
                    } else {
                        None
                    };
                    
                    if let Some(class_name) = resolved_class_name {
                        let visibility = if field_name == "ptr" {
                            "pub(crate)"
                        } else {
                            "pub"
                        };

                        // Check if we need wrapper postfix (for enums in Python bindings)
                        let mut field_postfix = config.wrapper_postfix.clone();
                        let prevent_wrapper_recursion = !config.wrapper_postfix.is_empty()
                            && struct_name.ends_with(&config.wrapper_postfix);

                        if let Some(found_class) = get_class(version_data, "", &class_name) {
                            let found_is_enum = found_class.enum_fields.is_some();
                            if !found_is_enum || prevent_wrapper_recursion {
                                field_postfix.clear();
                            }
                        }

                        code.push_str(&format!(
                            "{}    {} {}: {}{}{}{}{},\n",
                            indent_str,
                            visibility,
                            field_name,
                            prefix,
                            &config.prefix,
                            class_name,
                            field_postfix,
                            suffix
                        ));
                    } else {
                        // Type not found - use as-is
                        code.push_str(&format!(
                            "{}    pub {}: {},\n",
                            indent_str, field_name, field_type
                        ));
                    }
                }
            }
        }
    }

    code.push_str(&format!("{}}}\n\n", indent_str));

    // Generate trait implementations for custom_impls
    // These cast to the external type, call the trait method, and cast back if needed
    let external_path = struct_meta.external.as_deref().unwrap_or(struct_name);

    // Generate impl Clone if custom_impls contains "Clone"
    if struct_meta.custom_impls.contains(&"Clone".to_string()) {
        code.push_str(&format!(
            "{}impl Clone for {} {{\n",
            indent_str, struct_name
        ));
        code.push_str(&format!(
            "{}    fn clone(&self) -> Self {{\n",
            indent_str
        ));
        code.push_str(&format!(
            "{}        unsafe {{ {}_deepCopy(self) }}\n",
            indent_str, struct_name
        ));
        code.push_str(&format!("{}    }}\n", indent_str));
        code.push_str(&format!("{}}}\n\n", indent_str));
    }

    // Generate impl Drop if custom_impls contains "Drop"
    if struct_meta.custom_impls.contains(&"Drop".to_string()) {
        code.push_str(&format!(
            "{}impl Drop for {} {{\n",
            indent_str, struct_name
        ));
        code.push_str(&format!(
            "{}    fn drop(&mut self) {{\n",
            indent_str
        ));
        code.push_str(&format!(
            "{}        unsafe {{ {}_delete(self) }}\n",
            indent_str, struct_name
        ));
        code.push_str(&format!("{}    }}\n", indent_str));
        code.push_str(&format!("{}}}\n\n", indent_str));
    }

    // Generate impl Debug if custom_impls contains "Debug"
    if struct_meta.custom_impls.contains(&"Debug".to_string()) {
        code.push_str(&format!(
            "{}impl core::fmt::Debug for {} {{\n",
            indent_str, struct_name
        ));
        code.push_str(&format!(
            "{}    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {{\n",
            indent_str
        ));
        code.push_str(&format!(
            "{}        unsafe {{ (*(self as *const {} as *const {})).fmt(f) }}\n",
            indent_str, struct_name, external_path
        ));
        code.push_str(&format!("{}    }}\n", indent_str));
        code.push_str(&format!("{}}}\n\n", indent_str));
    }

    // Generate impl PartialEq if custom_impls contains "PartialEq"
    if struct_meta.custom_impls.contains(&"PartialEq".to_string()) {
        code.push_str(&format!(
            "{}impl PartialEq for {} {{\n",
            indent_str, struct_name
        ));
        code.push_str(&format!(
            "{}    fn eq(&self, other: &Self) -> bool {{\n",
            indent_str
        ));
        code.push_str(&format!(
            "{}        unsafe {{ (*(self as *const {} as *const {})).eq(&*(other as *const {} as *const {})) }}\n",
            indent_str, struct_name, external_path, struct_name, external_path
        ));
        code.push_str(&format!("{}    }}\n", indent_str));
        code.push_str(&format!("{}}}\n\n", indent_str));
    }

    // Generate impl PartialOrd if custom_impls contains "PartialOrd"
    if struct_meta.custom_impls.contains(&"PartialOrd".to_string()) {
        code.push_str(&format!(
            "{}impl PartialOrd for {} {{\n",
            indent_str, struct_name
        ));
        code.push_str(&format!(
            "{}    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {{\n",
            indent_str
        ));
        code.push_str(&format!(
            "{}        unsafe {{ (*(self as *const {} as *const {})).partial_cmp(&*(other as *const {} as *const {})) }}\n",
            indent_str, struct_name, external_path, struct_name, external_path
        ));
        code.push_str(&format!("{}    }}\n", indent_str));
        code.push_str(&format!("{}}}\n\n", indent_str));
    }

    // Generate impl Eq if custom_impls contains "Eq"
    if struct_meta.custom_impls.contains(&"Eq".to_string()) {
        code.push_str(&format!(
            "{}impl Eq for {} {{}}\n\n",
            indent_str, struct_name
        ));
    }

    // Generate impl Ord if custom_impls contains "Ord"
    if struct_meta.custom_impls.contains(&"Ord".to_string()) {
        code.push_str(&format!(
            "{}impl Ord for {} {{\n",
            indent_str, struct_name
        ));
        code.push_str(&format!(
            "{}    fn cmp(&self, other: &Self) -> core::cmp::Ordering {{\n",
            indent_str
        ));
        code.push_str(&format!(
            "{}        unsafe {{ (*(self as *const {} as *const {})).cmp(&*(other as *const {} as *const {})) }}\n",
            indent_str, struct_name, external_path, struct_name, external_path
        ));
        code.push_str(&format!("{}    }}\n", indent_str));
        code.push_str(&format!("{}}}\n\n", indent_str));
    }

    // Generate impl Hash if custom_impls contains "Hash"
    if struct_meta.custom_impls.contains(&"Hash".to_string()) {
        code.push_str(&format!(
            "{}impl core::hash::Hash for {} {{\n",
            indent_str, struct_name
        ));
        code.push_str(&format!(
            "{}    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {{\n",
            indent_str
        ));
        code.push_str(&format!(
            "{}        unsafe {{ (*(self as *const {} as *const {})).hash(state) }}\n",
            indent_str, struct_name, external_path
        ));
        code.push_str(&format!("{}    }}\n", indent_str));
        code.push_str(&format!("{}}}\n\n", indent_str));
    }

    Ok(code)
}

fn generate_enum_definition(
    version_data: &VersionData,
    struct_name: &str,
    struct_meta: &StructMetadata,
    enum_fields: &[IndexMap<String, EnumVariantData>],
    config: &GenerateConfig,
    indent_str: &str,
) -> Result<String> {
    let mut code = String::new();

    // Determine repr
    let mut repr = format!("{}#[repr(C)]\n", indent_str);
    for variant_map in enum_fields {
        for (_variant_name, variant_data) in variant_map {
            if variant_data.r#type.is_some() {
                repr = format!("{}#[repr(C, u8)]\n", indent_str);
                break;
            }
        }
    }

    if let Some(custom_repr) = &struct_meta.repr {
        repr = format!("{}#[repr({})]\n", indent_str, custom_repr);
    }

    // SIMPLIFIED: Use derives directly from api.json (struct_meta.derive)
    // All derive information is now explicit in api.json - no auto-computation
    // BUT: Serialize/Deserialize need special handling - they go in cfg_attr
    let mut derives: Vec<&str> = Vec::new();
    let mut has_serialize = false;
    let mut has_deserialize = false;
    
    if !config.no_derive {
        // Add derives from api.json, but filter out Serialize/Deserialize
        for d in &struct_meta.derive {
            match d.as_str() {
                "Serialize" => has_serialize = true,
                "Deserialize" => has_deserialize = true,
                other => derives.push(other),
            }
        }
    }

    // Generate derive attributes
    let derive_str = if derives.is_empty() {
        String::new()
    } else {
        format!("{}#[derive({})]\n", indent_str, derives.join(", "))
    };

    // Serde derives - use Serialize/Deserialize from api.json derives array
    let opt_derive_serde = if config.no_derive {
        String::new()
    } else if has_serialize && has_deserialize {
        format!(
            "{}#[cfg_attr(feature = \"serde-support\", derive(Serialize, Deserialize))]\n",
            indent_str
        )
    } else if has_serialize {
        format!(
            "{}#[cfg_attr(feature = \"serde-support\", derive(Serialize))]\n",
            indent_str
        )
    } else if has_deserialize {
        format!(
            "{}#[cfg_attr(feature = \"serde-support\", derive(Deserialize))]\n",
            indent_str
        )
    } else {
        String::new()
    };

    let opt_derive_serde_extra = if !config.no_derive && !opt_derive_serde.is_empty() {
        if let Some(serde_extra) = &struct_meta.serde_extra {
            format!(
                "{}#[cfg_attr(feature = \"serde-support\", serde({}))]\n",
                indent_str, serde_extra
            )
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    // Write all attributes
    code.push_str(&repr);
    code.push_str(&derive_str);
    code.push_str(&opt_derive_serde);
    code.push_str(&opt_derive_serde_extra);

    // Add generic parameters if present
    let generic_params_str = if let Some(params) = &struct_meta.generic_params {
        if params.is_empty() {
            String::new()
        } else {
            format!("<{}>", params.join(", "))
        }
    } else {
        String::new()
    };

    code.push_str(&format!(
        "{}pub enum {}{} {{\n",
        indent_str, struct_name, generic_params_str
    ));

    // Generate variants
    for variant_map in enum_fields {
        for (variant_name, variant_data) in variant_map {
            if let Some(variant_type) = &variant_data.r#type {
                // Check if this is a tuple type (multiple types separated by commas)
                if variant_type.contains(',') {
                    // Split by comma and prefix each type
                    let prefixed_types: Vec<String> = variant_type
                        .split(',')
                        .map(|t| {
                            let t = t.trim();
                            let (prefix, base_type, suffix) = analyze_type(t);
                            if is_primitive_arg(&base_type) {
                                format!("{}{}{}", prefix, base_type, suffix)
                            } else if let Some((_, class_name)) =
                                search_for_class_by_class_name(version_data, &base_type)
                            {
                                format!("{}{}{}{}", prefix, &config.prefix, class_name, suffix)
                            } else {
                                t.to_string()
                            }
                        })
                        .collect();
                    code.push_str(&format!(
                        "{}    {}({}),\n",
                        indent_str, variant_name, prefixed_types.join(", ")
                    ));
                    continue;
                }

                // Check if this is a generic type parameter (like T)
                let is_generic_param = struct_meta
                    .generic_params
                    .as_ref()
                    .map(|params| params.contains(&variant_type.to_string()))
                    .unwrap_or(false);

                if is_generic_param {
                    // Generic type parameter - use as-is
                    code.push_str(&format!(
                        "{}    {}({}),\n",
                        indent_str, variant_name, variant_type
                    ));
                } else if is_primitive_arg(variant_type) {
                    // Primitive variant
                    code.push_str(&format!(
                        "{}    {}({}),\n",
                        indent_str, variant_name, variant_type
                    ));
                } else {
                    let (prefix, base_type, suffix) = analyze_type(variant_type);

                    if is_primitive_arg(&base_type) {
                        // Array type like [f32; 4]
                        code.push_str(&format!(
                            "{}    {}({}{}{}),\n",
                            indent_str, variant_name, prefix, base_type, suffix
                        ));
                    } else {
                        // Complex type - resolve
                        if let Some((_, class_name)) =
                            search_for_class_by_class_name(version_data, &base_type)
                        {
                            let mut variant_postfix = config.wrapper_postfix.clone();

                            if let Some(found_class) = get_class(version_data, "", class_name) {
                                let found_is_enum = found_class.enum_fields.is_some();
                                if !found_is_enum {
                                    variant_postfix.clear();
                                }
                            }

                            code.push_str(&format!(
                                "{}    {}({}{}{}{}{}),\n",
                                indent_str,
                                variant_name,
                                prefix,
                                &config.prefix,
                                class_name,
                                variant_postfix,
                                suffix
                            ));
                        } else {
                            // Type not found - use as-is
                            code.push_str(&format!(
                                "{}    {}({}),\n",
                                indent_str, variant_name, variant_type
                            ));
                        }
                    }
                }
            } else {
                // Unit variant
                code.push_str(&format!("{}    {},\n", indent_str, variant_name));
            }
        }
    }

    code.push_str(&format!("{}}}\n\n", indent_str));

    Ok(code)
}

/// Prefix types in an extern "C" fn signature string
/// Takes a raw extern fn string like `extern "C" fn (&mut RefAny, &mut CallbackInfo) -> Update`
/// and returns it with all known types prefixed
fn prefix_types_in_extern_fn_string(
    version_data: &VersionData,
    fn_string: &str,
    prefix: &str,
) -> String {
    // Collect all known type names from api.json
    let mut known_types = std::collections::HashSet::new();
    for module_data in version_data.api.values() {
        for class_name in module_data.classes.keys() {
            known_types.insert(class_name.as_str());
        }
    }

    // Process the string, replacing known types with prefixed versions
    let mut result = fn_string.to_string();
    
    // Sort types by length descending to avoid partial matches (e.g., "String" before "Str")
    let mut types_vec: Vec<&str> = known_types.iter().copied().collect();
    types_vec.sort_by(|a, b| b.len().cmp(&a.len()));
    
    for type_name in types_vec {
        // Skip types that are likely not meant to be prefixed
        if type_name == "String" || type_name == "Vec" {
            continue;
        }
        
        // Skip primitive types
        if matches!(type_name, "bool" | "f32" | "f64" | "i8" | "i16" | "i32" | "i64" | "i128" 
            | "isize" | "u8" | "u16" | "u32" | "u64" | "u128" | "usize" | "c_void" | "char") {
            continue;
        }
        
        // Replace whole words only using word boundaries (\b)
        // Always add prefix, even if type already has it (consistency)
        let pattern = format!(r"\b{}\b", regex::escape(type_name));
        match regex::Regex::new(&pattern) {
            Ok(re) => {
                let replacement = format!("{}{}", prefix, type_name);
                result = re.replace_all(&result, replacement.as_str()).to_string();
            }
            Err(e) => {
                eprintln!("Warning: Failed to compile regex for {}: {:?}", type_name, e);
            }
        }
    }
    
    result
}

/// Generate a Rust callback function type definition
/// Example: `extern "C" fn(&AzDom, AzEventFilter) -> AzCallbackReturn`
fn generate_rust_callback_fn_type(
    version_data: &VersionData,
    callback_typedef: &crate::api::CallbackDefinition,
    prefix: &str,
) -> Result<String> {
    let mut fn_string = String::from("extern \"C\" fn(");

    // Generate function arguments
    let fn_args = &callback_typedef.fn_args;
    if !fn_args.is_empty() {
        let mut args = Vec::new();

        for fn_arg in fn_args {
            let fn_arg_type = &fn_arg.r#type;
            let fn_arg_ref = &fn_arg.ref_kind;

            let (_, base_type, _) = analyze_type(fn_arg_type);

            let mut arg_string = String::new();

            if !is_primitive_arg(&base_type) {
                // Complex type - need to find and prefix it
                if let Some((_, class_name)) =
                    search_for_class_by_class_name(version_data, &base_type)
                {
                    match fn_arg_ref.as_str() {
                        "ref" => arg_string = format!("&{}{}", prefix, class_name),
                        "refmut" => arg_string = format!("&mut {}{}", prefix, class_name),
                        "value" => arg_string = format!("{}{}", prefix, class_name),
                        _ => anyhow::bail!("Invalid fn_arg_ref: {}", fn_arg_ref),
                    }
                } else {
                    // Type not found - use as-is with prefix
                    eprintln!(
                        "Warning: Type {} not found in callback fn_arg, using as-is",
                        base_type
                    );
                    match fn_arg_ref.as_str() {
                        "ref" => arg_string = format!("&{}{}", prefix, base_type),
                        "refmut" => arg_string = format!("&mut {}{}", prefix, base_type),
                        "value" => arg_string = format!("{}{}", prefix, base_type),
                        _ => anyhow::bail!("Invalid fn_arg_ref: {}", fn_arg_ref),
                    }
                }
            } else {
                // Primitive type
                match fn_arg_ref.as_str() {
                    "ref" => arg_string = format!("&{}", fn_arg_type),
                    "refmut" => arg_string = format!("&mut {}", fn_arg_type),
                    "value" => arg_string = fn_arg_type.clone(),
                    _ => anyhow::bail!("Invalid fn_arg_ref: {}", fn_arg_ref),
                }
            }

            args.push(arg_string);
        }

        fn_string.push_str(&args.join(", "));
    }

    fn_string.push(')');

    // Generate return type
    if let Some(returns) = &callback_typedef.returns {
        fn_string.push_str(" -> ");

        let fn_ret_type = &returns.r#type;
        let (_, base_type, _) = analyze_type(fn_ret_type);

        if !is_primitive_arg(&base_type) {
            if let Some((_, class_name)) = search_for_class_by_class_name(version_data, &base_type)
            {
                fn_string.push_str(&format!("{}{}", prefix, class_name));
            } else {
                // Type not found - use as-is with prefix
                eprintln!(
                    "Warning: Return type {} not found in callback, using as-is",
                    base_type
                );
                fn_string.push_str(&format!("{}{}", prefix, base_type));
            }
        } else {
            fn_string.push_str(fn_ret_type);
        }
    }

    Ok(fn_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{ApiData, ModuleData};

    #[test]
    fn test_simple_struct_generation() {
        // Create a simple struct: Point { x: f32, y: f32 }
        let mut struct_fields = Vec::new();
        let mut field_map = IndexMap::new();
        field_map.insert(
            "x".to_string(),
            FieldData {
                r#type: "f32".to_string(),
                doc: None,
                derive: None,
            },
        );
        field_map.insert(
            "y".to_string(),
            FieldData {
                r#type: "f32".to_string(),
                doc: None,
                derive: None,
            },
        );
        struct_fields.push(field_map);

        let mut meta = StructMetadata {
            name: "Point".to_string(),
            doc: Some("A 2D point".to_string()),
            external: Some("crate::Point".to_string()),
            derive: vec!["Copy".to_string(), "Clone".to_string(), "Debug".to_string()],
            is_callback_typedef: false,
            can_be_copied: true,
            can_be_serde_serialized: false,
            can_be_serde_deserialized: false,
            implements_default: false,
            implements_eq: false,
            implements_ord: false,
            implements_hash: false,
            has_custom_destructor: false,
            can_be_cloned: true,
            is_boxed_object: false,
            treat_external_as_ptr: false,
            serde_extra: None,
            repr: None,
            struct_fields: Some(struct_fields),
            enum_fields: None,
            callback_typedef: None,
            type_alias: None,
            generic_params: None,
            custom_impls: Vec::new(),
            vec_ref_element_type: None,
            vec_ref_is_mut: false,
            has_explicit_derive: true,
        };

        let mut structs_map = HashMap::new();
        structs_map.insert("AzPoint".to_string(), meta);

        let version_data = VersionData {
            apiversion: 1,
            git: "test".to_string(),
            date: "2025-01-01".to_string(),
            examples: Vec::new(),
            notes: Vec::new(),
            api: IndexMap::new(),
        };

        let config = GenerateConfig::default();
        let result = generate_structs(&version_data, &structs_map, &config).unwrap();

        assert!(result.contains("/// A 2D point"));
        assert!(result.contains("#[repr(C)]"));
        assert!(result.contains("#[derive(Debug)]"));
        assert!(result.contains("#[derive(Clone)]"));
        assert!(result.contains("#[derive(Copy)]"));
        assert!(result.contains("pub struct AzPoint {"));
        assert!(result.contains("pub x: f32,"));
        assert!(result.contains("pub y: f32,"));
    }

    #[test]
    fn test_simple_enum_generation() {
        // Create a simple enum: Color { Red, Green, Blue }
        let mut enum_fields = Vec::new();
        let mut variant1 = IndexMap::new();
        variant1.insert(
            "Red".to_string(),
            EnumVariantData {
                r#type: None,
                doc: None,
            },
        );
        enum_fields.push(variant1);

        let mut variant2 = IndexMap::new();
        variant2.insert(
            "Green".to_string(),
            EnumVariantData {
                r#type: None,
                doc: None,
            },
        );
        enum_fields.push(variant2);

        let mut variant3 = IndexMap::new();
        variant3.insert(
            "Blue".to_string(),
            EnumVariantData {
                r#type: None,
                doc: None,
            },
        );
        enum_fields.push(variant3);

        let meta = StructMetadata {
            name: "Color".to_string(),
            doc: Some("RGB color".to_string()),
            external: Some("crate::Color".to_string()),
            derive: vec!["Copy".to_string(), "Clone".to_string(), "Debug".to_string()],
            is_callback_typedef: false,
            can_be_copied: true,
            can_be_serde_serialized: false,
            can_be_serde_deserialized: false,
            implements_default: false,
            implements_eq: false,
            implements_ord: false,
            implements_hash: false,
            has_custom_destructor: false,
            can_be_cloned: true,
            is_boxed_object: false,
            treat_external_as_ptr: false,
            serde_extra: None,
            repr: None,
            struct_fields: None,
            enum_fields: Some(enum_fields),
            callback_typedef: None,
            type_alias: None,
            generic_params: None,
            custom_impls: Vec::new(),
            vec_ref_element_type: None,
            vec_ref_is_mut: false,
            has_explicit_derive: true,
        };

        let mut structs_map = HashMap::new();
        structs_map.insert("AzColor".to_string(), meta);

        let version_data = VersionData {
            apiversion: 1,
            git: "test".to_string(),
            date: "2025-01-01".to_string(),
            examples: Vec::new(),
            notes: Vec::new(),
            api: IndexMap::new(),
        };

        let config = GenerateConfig::default();
        let result = generate_structs(&version_data, &structs_map, &config).unwrap();

        assert!(result.contains("/// RGB color"));
        assert!(result.contains("#[repr(C)]"));
        assert!(result.contains("#[derive(Debug)]"));
        assert!(result.contains("pub enum AzColor {"));
        assert!(result.contains("Red,"));
        assert!(result.contains("Green,"));
        assert!(result.contains("Blue,"));
    }

    #[test]
    fn test_enum_with_data() {
        // Create enum: Option { None, Some(i32) }
        let mut enum_fields = Vec::new();
        let mut variant1 = IndexMap::new();
        variant1.insert(
            "None".to_string(),
            EnumVariantData {
                r#type: None,
                doc: None,
            },
        );
        enum_fields.push(variant1);

        let mut variant2 = IndexMap::new();
        variant2.insert(
            "Some".to_string(),
            EnumVariantData {
                r#type: Some("i32".to_string()),
                doc: None,
            },
        );
        enum_fields.push(variant2);

        let meta = StructMetadata {
            name: "Option".to_string(),
            doc: None,
            external: Some("crate::Option".to_string()),
            derive: vec!["Copy".to_string(), "Clone".to_string()],
            is_callback_typedef: false,
            can_be_copied: true,
            can_be_serde_serialized: false,
            can_be_serde_deserialized: false,
            implements_default: false,
            implements_eq: false,
            implements_ord: false,
            implements_hash: false,
            has_custom_destructor: false,
            can_be_cloned: true,
            is_boxed_object: false,
            treat_external_as_ptr: false,
            serde_extra: None,
            repr: None,
            struct_fields: None,
            enum_fields: Some(enum_fields),
            callback_typedef: None,
            type_alias: None,
            generic_params: None,
            custom_impls: Vec::new(),
            vec_ref_element_type: None,
            vec_ref_is_mut: false,
            has_explicit_derive: true,
        };

        let mut structs_map = HashMap::new();
        structs_map.insert("AzOption".to_string(), meta);

        let version_data = VersionData {
            apiversion: 1,
            git: "test".to_string(),
            date: "2025-01-01".to_string(),
            examples: Vec::new(),
            notes: Vec::new(),
            api: IndexMap::new(),
        };

        let config = GenerateConfig::default();
        let result = generate_structs(&version_data, &structs_map, &config).unwrap();

        assert!(
            result.contains("#[repr(C, u8)]"),
            "Should use repr(C, u8) for enums with data"
        );
        assert!(result.contains("pub enum AzOption {"));
        assert!(result.contains("None,"));
        assert!(result.contains("Some(i32),"));
    }
}
