//! Struct, Enum, and Typedef generation
//!
//! This module generates Rust struct/enum definitions from api.json data.
//! It's a direct port of the `generate_structs` function from oldbuild.py.

use std::collections::HashMap;

use anyhow::Result;
use indexmap::IndexMap;

use crate::{
    api::{ApiData, ClassData, EnumVariantData, FieldData, VersionData},
    utils::analyze::{analyze_type, get_class, is_primitive_arg, search_for_class_by_class_name},
};

/// Configuration for struct generation
#[derive(Debug, Clone)]
pub struct GenerateConfig {
    /// Type prefix (e.g., "Az", "Az1", "Az2" based on version)
    pub prefix: String,
    /// Indentation level (number of spaces)
    pub indent: usize,
    /// Whether to auto-derive Debug, Clone, etc.
    pub autoderive: bool,
    /// Whether to make pointer fields pub(crate) instead of pub
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
            autoderive: true,
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
}

impl StructMetadata {
    /// Extract metadata from ClassData
    pub fn from_class_data(name: String, class_data: &ClassData) -> Self {
        let derive = class_data.derive.clone().unwrap_or_default();

        let is_callback_typedef = class_data.callback_typedef.is_some()
            && !class_data
                .callback_typedef
                .as_ref()
                .unwrap()
                .fn_args
                .is_empty();
        let can_be_copied = derive.contains(&"Copy".to_string());
        let can_be_serde_serialized = derive.contains(&"Serialize".to_string());
        let can_be_serde_deserialized = derive.contains(&"Deserialize".to_string());
        let implements_default = derive.contains(&"Default".to_string());
        let implements_eq = derive.contains(&"Eq".to_string());
        let implements_ord = derive.contains(&"Ord".to_string());
        let implements_hash = derive.contains(&"Hash".to_string());

        let has_custom_destructor = class_data.custom_destructor.unwrap_or(false);
        let can_be_cloned = class_data.clone.unwrap_or(true);
        let is_boxed_object = class_data.is_boxed_object;
        let treat_external_as_ptr = class_data.external.is_some() && is_boxed_object;

        Self {
            name,
            doc: class_data.doc.clone(),
            external: class_data.external.clone(),
            derive,
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

    for (struct_name, struct_meta) in structs_map {
        code.push_str(&generate_single_type(
            version_data,
            struct_name,
            struct_meta,
            config,
            &indent_str,
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
) -> Result<String> {
    let mut code = String::new();

    // Add documentation
    if let Some(doc) = &struct_meta.doc {
        code.push_str(&format!("{}/// {}\n", indent_str, doc));
    } else {
        code.push_str(&format!("{}/// `{}` struct\n", indent_str, struct_name));
    }

    // Handle callback typedefs
    if struct_meta.is_callback_typedef {
        if let Some(callback_typedef) = &struct_meta.callback_typedef {
            let fn_ptr = generate_rust_callback_fn_type(callback_typedef);
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
        )?);
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

fn generate_struct_definition(
    version_data: &VersionData,
    struct_name: &str,
    struct_meta: &StructMetadata,
    struct_fields: &[IndexMap<String, FieldData>],
    config: &GenerateConfig,
    indent_str: &str,
) -> Result<String> {
    let mut code = String::new();

    // Determine derives
    let mut opt_derive_debug = if !config.no_derive {
        format!("{}#[derive(Debug)]\n", indent_str)
    } else {
        String::new()
    };
    let mut opt_derive_clone = if !config.no_derive {
        format!("{}#[derive(Clone)]\n", indent_str)
    } else {
        String::new()
    };
    let mut opt_derive_copy = if !config.no_derive {
        format!("{}#[derive(Copy)]\n", indent_str)
    } else {
        String::new()
    };
    let mut opt_derive_other = if !config.no_derive {
        format!("{}#[derive(PartialEq, PartialOrd)]\n", indent_str)
    } else {
        String::new()
    };
    let mut opt_derive_eq = String::new();
    let mut opt_derive_ord = String::new();
    let mut opt_derive_hash = String::new();

    // Apply derive rules
    if !struct_meta.can_be_copied {
        opt_derive_copy.clear();
    }

    if !struct_meta.can_be_cloned
        || (struct_meta.treat_external_as_ptr && struct_meta.can_be_cloned)
    {
        opt_derive_clone.clear();
    }

    if struct_name == "AzString" {
        opt_derive_debug.clear();
    }

    if struct_meta.has_custom_destructor || !config.autoderive || struct_name == "AzU8VecRef" {
        opt_derive_copy.clear();
        opt_derive_debug.clear();
        opt_derive_clone.clear();
        opt_derive_other.clear();
    }

    if !opt_derive_other.is_empty() {
        if struct_meta.implements_eq {
            opt_derive_eq = format!("{}#[derive(Eq)]\n", indent_str);
        }
        if struct_meta.implements_ord {
            opt_derive_ord = format!("{}#[derive(Ord)]\n", indent_str);
        }
        if struct_meta.implements_hash {
            opt_derive_hash = format!("{}#[derive(Hash)]\n", indent_str);
        }
    }

    // Check if any field contains a callback (removes Debug)
    for field_map in struct_fields {
        for (_field_name, field_data) in field_map {
            let field_type = &field_data.r#type;
            let (_, base_type, _) = analyze_type(field_type);

            if !is_primitive_arg(&base_type) {
                if let Some((module_name, class_name)) =
                    search_for_class_by_class_name(version_data, &base_type)
                {
                    if let Some(found_class) = get_class(version_data, module_name, class_name) {
                        let found_is_callback = found_class.callback_typedef.is_some()
                            && !found_class
                                .callback_typedef
                                .as_ref()
                                .unwrap()
                                .fn_args
                                .is_empty();
                        if found_is_callback {
                            opt_derive_debug.clear();
                            opt_derive_other.clear();
                        }
                    }
                }
            }
        }
    }

    // Serde derives
    let opt_derive_default = if struct_meta.implements_default && !config.no_derive {
        format!("{}#[derive(Default)]\n", indent_str)
    } else {
        String::new()
    };

    let opt_derive_serde = if config.no_derive {
        String::new()
    } else if struct_meta.can_be_serde_serialized && struct_meta.can_be_serde_deserialized {
        format!(
            "{}#[cfg_attr(feature = \"serde-support\", derive(Serialize, Deserialize))]\n",
            indent_str
        )
    } else if struct_meta.can_be_serde_serialized {
        format!(
            "{}#[cfg_attr(feature = \"serde-support\", derive(Serialize))]\n",
            indent_str
        )
    } else if struct_meta.can_be_serde_deserialized {
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
    code.push_str(&opt_derive_debug);
    code.push_str(&opt_derive_clone);
    code.push_str(&opt_derive_other);
    code.push_str(&opt_derive_copy);
    code.push_str(&opt_derive_eq);
    code.push_str(&opt_derive_ord);
    code.push_str(&opt_derive_hash);
    code.push_str(&opt_derive_default);
    code.push_str(&opt_derive_serde);
    code.push_str(&opt_derive_serde_extra);
    code.push_str(&format!("{}pub struct {} {{\n", indent_str, struct_name));

    // Generate fields
    for field_map in struct_fields {
        for (field_name, field_data) in field_map {
            let field_type = &field_data.r#type;
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
                if let Some((_, class_name)) =
                    search_for_class_by_class_name(version_data, &base_type)
                {
                    let visibility = if field_name == "ptr" {
                        "pub(crate)"
                    } else {
                        "pub"
                    };

                    // Check if we need wrapper postfix (for enums in Python bindings)
                    let mut field_postfix = config.wrapper_postfix.clone();
                    let prevent_wrapper_recursion = !config.wrapper_postfix.is_empty()
                        && struct_name.ends_with(&config.wrapper_postfix);

                    if let Some(found_class) = get_class(version_data, "", class_name) {
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

    code.push_str(&format!("{}}}\n\n", indent_str));

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

    // Determine derives (same logic as structs)
    let mut opt_derive_debug = if !config.no_derive {
        format!("{}#[derive(Debug)]\n", indent_str)
    } else {
        String::new()
    };
    let mut opt_derive_clone = if !config.no_derive {
        format!("{}#[derive(Clone)]\n", indent_str)
    } else {
        String::new()
    };
    let mut opt_derive_copy = if !config.no_derive {
        format!("{}#[derive(Copy)]\n", indent_str)
    } else {
        String::new()
    };
    let mut opt_derive_other = if !config.no_derive {
        format!("{}#[derive(PartialEq, PartialOrd)]\n", indent_str)
    } else {
        String::new()
    };
    let mut opt_derive_eq = String::new();
    let mut opt_derive_ord = String::new();
    let mut opt_derive_hash = String::new();

    if !struct_meta.can_be_copied {
        opt_derive_copy.clear();
    }

    if !struct_meta.can_be_cloned
        || (struct_meta.treat_external_as_ptr && struct_meta.can_be_cloned)
    {
        opt_derive_clone.clear();
    }

    if struct_meta.has_custom_destructor || !config.autoderive {
        opt_derive_copy.clear();
        opt_derive_debug.clear();
        opt_derive_clone.clear();
        opt_derive_other.clear();
    }

    if !opt_derive_other.is_empty() {
        if struct_meta.implements_eq {
            opt_derive_eq = format!("{}#[derive(Eq)]\n", indent_str);
        }
        if struct_meta.implements_ord {
            opt_derive_ord = format!("{}#[derive(Ord)]\n", indent_str);
        }
        if struct_meta.implements_hash {
            opt_derive_hash = format!("{}#[derive(Hash)]\n", indent_str);
        }
    }

    // Check if any variant contains a callback
    for variant_map in enum_fields {
        for (_variant_name, variant_data) in variant_map {
            if let Some(variant_type) = &variant_data.r#type {
                let (_, base_type, _) = analyze_type(variant_type);

                if !is_primitive_arg(&base_type) {
                    if let Some((module_name, class_name)) =
                        search_for_class_by_class_name(version_data, &base_type)
                    {
                        if let Some(found_class) = get_class(version_data, module_name, class_name)
                        {
                            let found_is_callback = found_class.callback_typedef.is_some()
                                && !found_class
                                    .callback_typedef
                                    .as_ref()
                                    .unwrap()
                                    .fn_args
                                    .is_empty();
                            if found_is_callback {
                                opt_derive_debug.clear();
                                opt_derive_other.clear();
                            }
                        }
                    }
                }
            }
        }
    }

    // Serde derives
    let opt_derive_default = if struct_meta.implements_default && !config.no_derive {
        format!("{}#[derive(Default)]\n", indent_str)
    } else {
        String::new()
    };

    let opt_derive_serde = if config.no_derive {
        String::new()
    } else if struct_meta.can_be_serde_serialized && struct_meta.can_be_serde_deserialized {
        format!(
            "{}#[cfg_attr(feature = \"serde-support\", derive(Serialize, Deserialize))]\n",
            indent_str
        )
    } else if struct_meta.can_be_serde_serialized {
        format!(
            "{}#[cfg_attr(feature = \"serde-support\", derive(Serialize))]\n",
            indent_str
        )
    } else if struct_meta.can_be_serde_deserialized {
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
    code.push_str(&opt_derive_debug);
    code.push_str(&opt_derive_clone);
    code.push_str(&opt_derive_other);
    code.push_str(&opt_derive_copy);
    code.push_str(&opt_derive_ord);
    code.push_str(&opt_derive_eq);
    code.push_str(&opt_derive_hash);
    code.push_str(&opt_derive_default);
    code.push_str(&opt_derive_serde);
    code.push_str(&opt_derive_serde_extra);
    code.push_str(&format!("{}pub enum {} {{\n", indent_str, struct_name));

    // Generate variants
    for variant_map in enum_fields {
        for (variant_name, variant_data) in variant_map {
            if let Some(variant_type) = &variant_data.r#type {
                if is_primitive_arg(variant_type) {
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

fn generate_rust_callback_fn_type(callback_typedef: &crate::api::CallbackDefinition) -> String {
    // Simplified callback generation - in full implementation, parse callback_typedef properly
    // For now, return a generic fn pointer
    "extern \"C\" fn()".to_string()
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
