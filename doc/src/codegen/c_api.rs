use std::collections::HashMap;

use anyhow::{bail, Result};
use indexmap::IndexMap;

use crate::{
    api::{ApiData, CallbackDefinition, ClassData},
    autofix::types::ref_kind::RefKind,
    utils::{
        analyze::{
            analyze_type, class_is_stack_allocated, enum_is_union, has_recursive_destructor,
            is_primitive_arg, replace_primitive_ctype, search_for_class_by_class_name,
        },
        string::snake_case_to_lower_camel,
    },
};

const PREFIX: &str = "Az";

/// Generate a C function pointer typedef from a CallbackDefinition
/// 
/// Example output: `typedef AzUpdate (*AzCallbackType)(AzRefAny* restrict, AzCallbackInfo* restrict);`
/// 
/// Note: `callback_name` already has the "Az" prefix from the sorted structs list.
fn format_c_callback_typedef(
    callback_name: &str,
    callback_def: &CallbackDefinition,
) -> String {
    // Determine return type
    let return_type = if let Some(ret) = &callback_def.returns {
        let base = &ret.r#type;
        if is_primitive_arg(base) {
            replace_primitive_ctype(base).to_string()
        } else {
            format!("{}{}", PREFIX, base)
        }
    } else {
        "void".to_string()
    };
    
    // Build argument list
    let args: Vec<String> = callback_def.fn_args.iter().map(|arg| {
        let base_type = &arg.r#type;
        let c_type = if is_primitive_arg(base_type) {
            replace_primitive_ctype(base_type).to_string()
        } else {
            format!("{}{}", PREFIX, base_type)
        };
        
        // Convert ref_kind to C pointer syntax
        match arg.ref_kind {
            RefKind::Ref => format!("const {}*", c_type),
            RefKind::RefMut => format!("{}* restrict", c_type),
            RefKind::ConstPtr => format!("const {}*", c_type),
            RefKind::MutPtr => format!("{}*", c_type),
            RefKind::Value => c_type,
            RefKind::Boxed | RefKind::OptionBoxed => format!("{}*", c_type),
        }
    }).collect();
    
    let args_str = if args.is_empty() {
        "void".to_string()
    } else {
        args.join(", ")
    };
    
    // callback_name already has the Az prefix from sorted structs
    format!("typedef {} (*{})({});\r\n\r\n", return_type, callback_name, args_str)
}

/// Extract array info from a type string for code generation
/// Returns (base_type, c_array_suffix) where c_array_suffix is like "[4]" for arrays
fn extract_array_from_type(type_str: &str) -> (String, String) {
    let trimmed = type_str.trim();
    
    // Check if it's an array type: [T; N]
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        let inner = &trimmed[1..trimmed.len() - 1];
        if let Some(semicolon_pos) = inner.rfind(';') {
            let base_type = inner[..semicolon_pos].trim().to_string();
            let size_str = inner[semicolon_pos + 1..].trim();
            if size_str.parse::<usize>().is_ok() {
                return (base_type, format!("[{}]", size_str));
            }
        }
    }
    
    (trimmed.to_string(), String::new())
}

/// Convert Rust array suffix to C array suffix
/// Rust: `; 20]` -> C: `[20]`
fn convert_array_suffix_to_c(rust_suffix: &str) -> String {
    if rust_suffix.is_empty() {
        return String::new();
    }
    // Format is "; N]" from analyze_type
    if rust_suffix.starts_with(';') && rust_suffix.ends_with(']') {
        let num_str = rust_suffix[1..rust_suffix.len()-1].trim();
        return format!("[{}]", num_str);
    }
    // Fallback: just use it as-is (shouldn't happen)
    rust_suffix.to_string()
}

/// Convert a RefKind to C pointer syntax (prefix, suffix)
/// Returns (prefix, suffix) where:
/// - prefix: e.g., "const " for const pointers
/// - suffix: e.g., "*" for pointers, "* restrict" for restrict pointers
fn ref_kind_to_c_syntax(ref_kind: &RefKind) -> (&'static str, &'static str) {
    match ref_kind {
        RefKind::Ref => ("const ", "*"),
        RefKind::RefMut => ("", "* restrict"),
        RefKind::ConstPtr => ("const ", "*"),
        RefKind::MutPtr => ("", "*"),
        RefKind::Value => ("", ""),
        RefKind::Boxed | RefKind::OptionBoxed => ("", "*"),
    }
}

/// Generate a monomorphized type from a generic type alias
/// E.g., CaretColorValue = CssPropertyValue<CaretColor> becomes a union
fn generate_monomorphized_type(
    code: &mut String,
    struct_name: &str,
    type_alias: &crate::api::TypeAliasInfo,
    target_class: &ClassData,
    version_data: &crate::api::VersionData,
) {
    // For CssPropertyValue<T>, the target is an enum with variants like:
    // Auto, None, Inherit, Initial, Exact(T)
    if let Some(enum_fields) = &target_class.enum_fields {
        if enum_is_union(enum_fields) {
            // Generate tag enum (use _Tag suffix to avoid clashing with standalone enums)
            code.push_str(&format!("enum {}_Tag {{\r\n", struct_name));
            for variant_map in enum_fields {
                for (variant_name, _) in variant_map {
                    code.push_str(&format!("   {}_Tag_{},\r\n", struct_name, variant_name));
                }
            }
            code.push_str("};\r\n");
            code.push_str(&format!("typedef enum {}_Tag {}_Tag;\r\n\r\n", struct_name, struct_name));
            
            // Generate variant structs
            for variant_map in enum_fields {
                for (variant_name, variant_data) in variant_map {
                    code.push_str(&format!("struct {}Variant_{} {{\r\n", struct_name, variant_name));
                    code.push_str(&format!("    {}_Tag tag;\r\n", struct_name));
                    
                    if let Some(variant_type) = &variant_data.r#type {
                        // Extract array info and substitute generic type parameter
                        let (base_type, array_suffix) = extract_array_from_type(variant_type);
                        let concrete_type = if is_generic_type_param(&base_type) {
                            // Replace T with the first generic arg
                            if let Some(arg) = type_alias.generic_args.first() {
                                arg.clone()
                            } else {
                                base_type
                            }
                        } else {
                            base_type
                        };
                        
                        if is_primitive_arg(&concrete_type) {
                            let c_type = replace_primitive_ctype(&concrete_type);
                            code.push_str(&format!("    {} payload{};\r\n", c_type, array_suffix));
                        } else {
                            code.push_str(&format!("    {}{} payload{};\r\n", PREFIX, concrete_type, array_suffix));
                        }
                    }
                    
                    code.push_str("};\r\n");
                    code.push_str(&format!("typedef struct {}Variant_{} {}Variant_{};\r\n\r\n", 
                        struct_name, variant_name, struct_name, variant_name));
                }
            }
            
            // Generate the union itself
            code.push_str(&format!("union {} {{\r\n", struct_name));
            for variant_map in enum_fields {
                for (variant_name, _) in variant_map {
                    code.push_str(&format!("    {}Variant_{} {};\r\n", struct_name, variant_name, variant_name));
                }
            }
            code.push_str("};\r\n\r\n");
        } else {
            // Simple enum - just generate an enum with substituted names
            code.push_str(&format!("enum {} {{\r\n", struct_name));
            for variant_map in enum_fields {
                for (variant_name, _) in variant_map {
                    code.push_str(&format!("   {}_{},\r\n", struct_name, variant_name));
                }
            }
            code.push_str("};\r\n\r\n");
        }
    } else if let Some(struct_fields) = &target_class.struct_fields {
        // Struct monomorphization - substitute generic type params
        code.push_str(&format!("struct {} {{\r\n", struct_name));
        for field_map in struct_fields {
            for (field_name, field_data) in field_map {
                let field_type = &field_data.r#type;
                let (c_ptr_prefix, c_ptr_suffix) = ref_kind_to_c_syntax(&field_data.ref_kind);
                let array_suffix = field_data.arraysize
                    .map(|n| format!("[{}]", n))
                    .unwrap_or_default();
                
                // Substitute generic type parameter
                let concrete_type = if is_generic_type_param(field_type) {
                    if let Some(arg) = type_alias.generic_args.first() {
                        arg.clone()
                    } else {
                        field_type.clone()
                    }
                } else {
                    field_type.clone()
                };
                
                if is_primitive_arg(&concrete_type) {
                    let c_type = replace_primitive_ctype(&concrete_type);
                    code.push_str(&format!("    {}{}{} {}{};\r\n", 
                        c_ptr_prefix, c_type, c_ptr_suffix, field_name, array_suffix));
                } else {
                    code.push_str(&format!("    {}{}{}{} {}{};\r\n", 
                        c_ptr_prefix, PREFIX, concrete_type, c_ptr_suffix, field_name, array_suffix));
                }
            }
        }
        code.push_str("};\r\n\r\n");
    }
}

/// Generate a tagged union (Rust enum with data) in C
fn generate_tagged_union(
    code: &mut String,
    struct_name: &str,
    enum_fields: &Vec<IndexMap<String, crate::api::EnumVariantData>>,
    version_data: &crate::api::VersionData,
) {
    // Generate tag enum (use _Tag suffix to avoid clashing with standalone enums)
    code.push_str(&format!("enum {}_Tag {{\r\n", struct_name));
    for variant_map in enum_fields {
        for (variant_name, _) in variant_map {
            code.push_str(&format!("   {}_Tag_{},\r\n", struct_name, variant_name));
        }
    }
    code.push_str("};\r\n");
    code.push_str(&format!("typedef enum {}_Tag {}_Tag;\r\n\r\n", struct_name, struct_name));

    // Generate variant structs
    for variant_map in enum_fields {
        for (variant_name, variant_data) in variant_map {
            code.push_str(&format!("struct {}Variant_{} {{\r\n", struct_name, variant_name));
            code.push_str(&format!("    {}_Tag tag;\r\n", struct_name));
            
            if let Some(variant_type) = &variant_data.r#type {
                let (base_type, array_suffix) = extract_array_from_type(variant_type);
                if is_primitive_arg(&base_type) {
                    let c_type = replace_primitive_ctype(&base_type);
                    code.push_str(&format!("    {} payload{};\r\n", c_type, array_suffix));
                } else if let Some((_, type_class_name)) = search_for_class_by_class_name(version_data, &base_type) {
                    code.push_str(&format!("    {}{} payload{};\r\n", PREFIX, type_class_name, array_suffix));
                }
            }
            
            code.push_str("};\r\n");
            code.push_str(&format!("typedef struct {}Variant_{} {}Variant_{};\r\n\r\n", 
                struct_name, variant_name, struct_name, variant_name));
        }
    }

    // Generate the union itself
    code.push_str(&format!("union {} {{\r\n", struct_name));
    for variant_map in enum_fields {
        for (variant_name, _) in variant_map {
            code.push_str(&format!("    {}Variant_{} {};\r\n", struct_name, variant_name, variant_name));
        }
    }
    code.push_str("};\r\n\r\n");
}

/// Generate C function arguments for a function/constructor
fn format_c_function_args(
    api_data: &ApiData,
    version: &str,
    function_data: &crate::api::FunctionData,
    class_name: &str,
    class_ptr_name: &str,
    self_as_first_arg: bool,
) -> String {
    let mut args = Vec::new();

    // Handle self parameter if needed
    if self_as_first_arg {
        if let Some(first_arg) = function_data.fn_args.first() {
            if let Some((arg_name, self_type)) = first_arg.iter().next() {
                if arg_name == "self" {
                    let class_lower = class_name.to_lowercase();

                    match self_type.as_str() {
                        "value" => {
                            args.push(format!("const {} {}", class_ptr_name, class_lower));
                        }
                        "mut value" => {
                            args.push(format!("{}* restrict {}", class_ptr_name, class_lower));
                        }
                        "refmut" => {
                            args.push(format!("{}* restrict {}", class_ptr_name, class_lower));
                        }
                        "ref" => {
                            args.push(format!("const {}* {}", class_ptr_name, class_lower));
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // Handle other arguments
    for arg in &function_data.fn_args {
        if let Some((arg_name, arg_type)) = arg.iter().next() {
            if arg_name == "self" {
                continue; // Skip self, already handled
            }

            let (prefix_ptr, base_type, _suffix) = analyze_type(arg_type);

            if is_primitive_arg(&base_type) {
                let c_type = replace_primitive_ctype(&base_type);

                if prefix_ptr == "*const " || prefix_ptr == "&" {
                    args.push(format!("const {}* {}", c_type, arg_name));
                } else if prefix_ptr == "*mut " || prefix_ptr == "&mut " {
                    args.push(format!("{}* restrict {}", c_type, arg_name));
                } else {
                    args.push(format!("{} {}", c_type, arg_name));
                }
            } else {
                // Non-primitive type - add PREFIX
                let c_type = format!("{}{}", PREFIX, replace_primitive_ctype(&base_type));
                let ptr_suffix = if prefix_ptr == "*const " || prefix_ptr == "&" {
                    "* "
                } else if prefix_ptr == "*mut " || prefix_ptr == "&mut " {
                    "* restrict "
                } else {
                    " "
                };

                args.push(format!("{}{}{}", c_type, ptr_suffix, arg_name));
            }
        }
    }

    args.join(", ")
}

/// Generate C API code from API data
pub fn generate_c_api(api_data: &ApiData, version: &str) -> String {
    let mut code = String::new();

    let version_data = api_data.get_version(version).unwrap();

    // Start C header file
    code.push_str("#ifndef AZUL_H\r\n");
    code.push_str("#define AZUL_H\r\n");
    code.push_str("\r\n");
    code.push_str("#include <stdbool.h>\r\n"); // bool
    code.push_str("#include <stdint.h>\r\n"); // uint8_t, ...
    code.push_str("#include <stddef.h>\r\n"); // size_t
    code.push_str("\r\n");

    // Add restrict keyword definitions for C89 compatibility
    code.push_str("/* C89 port for \"restrict\" keyword from C99 */\r\n");
    code.push_str("#if __STDC__ != 1\r\n");
    code.push_str("#    define restrict __restrict\r\n");
    code.push_str("#else\r\n");
    code.push_str("#    ifndef __STDC_VERSION__\r\n");
    code.push_str("#        define restrict __restrict\r\n");
    code.push_str("#    else\r\n");
    code.push_str("#        if __STDC_VERSION__ < 199901L\r\n");
    code.push_str("#            define restrict __restrict\r\n");
    code.push_str("#        endif\r\n");
    code.push_str("#    endif\r\n");
    code.push_str("#endif\r\n");
    code.push_str("\r\n");

    // Add cross-platform ssize_t definition
    code.push_str("/* cross-platform define for ssize_t (signed size_t) */\r\n");
    code.push_str("#ifdef _WIN32\r\n");
    code.push_str("    #include <windows.h>\r\n");
    code.push_str("    #ifdef _MSC_VER\r\n");
    code.push_str("        typedef SSIZE_T ssize_t;\r\n");
    code.push_str("    #endif\r\n");
    code.push_str("#else\r\n");
    code.push_str("    #include <sys/types.h>\r\n");
    code.push_str("#endif\r\n");
    code.push_str("\r\n");

    // Add cross-platform dllimport definition
    code.push_str("/* cross-platform define for __declspec(dllimport) */\r\n");
    code.push_str("#ifdef _WIN32\r\n");
    code.push_str("    #define DLLIMPORT __declspec(dllimport)\r\n");
    code.push_str("#else\r\n");
    code.push_str("    #define DLLIMPORT\r\n");
    code.push_str("#endif\r\n");
    code.push_str("\r\n");

    // Sort structs by dependencies (topological sort)
    // This ensures types are declared before they are used
    let sorted = sort_structs_by_dependencies(api_data, version, PREFIX)
        .expect("Failed to sort structs by dependencies");
    let structs = sorted.structs;

    // Collect callbacks for later
    let mut callbacks: Vec<(&String, &CallbackDefinition)> = Vec::new();
    
    // Phase 1: Forward declarations for ALL types (needed for recursive references)
    code.push_str("/* FORWARD DECLARATIONS */\r\n\r\n");
    for (struct_name, class_data) in &structs {
        if class_data.callback_typedef.is_some() {
            continue; // Skip callbacks
        }
        
        // If it has struct_fields, it's definitely a struct (even if it also has type_alias)
        if class_data.struct_fields.is_some() {
            code.push_str(&format!("struct {};\r\n", struct_name));
            code.push_str(&format!("typedef struct {} {};\r\n", struct_name, struct_name));
            continue;
        }
        
        // Type aliases with generics (and no struct_fields) need to check the target type
        if let Some(type_alias) = &class_data.type_alias {
            if !type_alias.generic_args.is_empty() {
                // Look up the target type to determine if it's a struct or enum
                let target = &type_alias.target;
                if let Some((_, target_class)) = search_for_class_by_class_name(version_data, target) {
                    if let Some(target_data) = version_data.api.values()
                        .find_map(|m| m.classes.get(target_class))
                    {
                        if target_data.struct_fields.is_some() {
                            // Target is a struct, so monomorphized type is also a struct
                            code.push_str(&format!("struct {};\r\n", struct_name));
                            code.push_str(&format!("typedef struct {} {};\r\n", struct_name, struct_name));
                            continue;
                        } else if target_data.enum_fields.is_some() {
                            // Target is an enum - check if it's a union (enum with data)
                            let is_union = target_data.enum_fields.as_ref()
                                .map(|f| enum_is_union(f))
                                .unwrap_or(false);
                            if is_union {
                                code.push_str(&format!("union {};\r\n", struct_name));
                                code.push_str(&format!("typedef union {} {};\r\n", struct_name, struct_name));
                            } else {
                                code.push_str(&format!("enum {};\r\n", struct_name));
                                code.push_str(&format!("typedef enum {} {};\r\n", struct_name, struct_name));
                            }
                            continue;
                        }
                    }
                }
                // Fallback: if target not found, assume union for backwards compatibility
                code.push_str(&format!("union {};\r\n", struct_name));
                code.push_str(&format!("typedef union {} {};\r\n", struct_name, struct_name));
                continue;
            }
            // Simple type aliases don't need forward declarations
            continue;
        }
        
        if class_data.enum_fields.is_some() {
            let is_union = class_data.enum_fields.as_ref()
                .map(|f| enum_is_union(f))
                .unwrap_or(false);
            if is_union {
                code.push_str(&format!("union {};\r\n", struct_name));
                code.push_str(&format!("typedef union {} {};\r\n", struct_name, struct_name));
            } else {
                code.push_str(&format!("enum {};\r\n", struct_name));
                code.push_str(&format!("typedef enum {} {};\r\n", struct_name, struct_name));
            }
        }
    }
    code.push_str("\r\n");
    
    // Phase 2: Type aliases (simple typedefs that reference other types)
    // Skip types that have struct_fields - they will be generated as actual structs
    code.push_str("/* TYPE ALIASES */\r\n\r\n");
    for (struct_name, class_data) in &structs {
        // Skip if it has struct_fields - it's a real struct, not just a typedef
        if class_data.struct_fields.is_some() {
            continue;
        }
        
        if let Some(type_alias) = &class_data.type_alias {
            let target = &type_alias.target;
            let ref_kind = &type_alias.ref_kind;
            
            if is_primitive_arg(target) {
                // Simple primitive alias (like CoreCallbackType -> usize)
                let c_type = replace_primitive_ctype(target);
                // Apply ref_kind for pointer types
                let (c_ptr_prefix, c_ptr_suffix) = ref_kind_to_c_syntax(ref_kind);
                code.push_str(&format!("typedef {}{}{} {};\r\n", c_ptr_prefix, c_type, c_ptr_suffix, struct_name));
            } else if type_alias.generic_args.is_empty() {
                // Non-generic type alias - typedef to the target struct
                // Apply ref_kind for pointer types (e.g., HwndHandle = *mut c_void)
                let (c_ptr_prefix, c_ptr_suffix) = ref_kind_to_c_syntax(ref_kind);
                code.push_str(&format!("typedef {}{}{}{} {};\r\n", c_ptr_prefix, PREFIX, target, c_ptr_suffix, struct_name));
            }
            // Generic type aliases are handled in the main definition loop
        }
    }
    code.push_str("\r\n");

    // Phase 3: Callback typedefs
    code.push_str("/* CALLBACK TYPEDEFS */\r\n\r\n");
    for (struct_name, class_data) in &structs {
        if let Some(callback_def) = &class_data.callback_typedef {
            code.push_str(&format_c_callback_typedef(struct_name, callback_def));
            callbacks.push((struct_name, callback_def));
        }
    }

    // Phase 4: All type definitions in dependency order
    // Since types are sorted by chain length, dependencies come first
    code.push_str("/* TYPE DEFINITIONS (sorted by dependency depth) */\r\n\r\n");

    for (struct_name, class_data) in &structs {
        // Skip callbacks (already handled)
        if class_data.callback_typedef.is_some() {
            continue;
        }
        
        // If it has struct_fields, generate as struct (ignore type_alias)
        if class_data.struct_fields.is_some() {
            // Will be handled below
        } else if let Some(type_alias) = &class_data.type_alias {
            // Only handle type_alias if there are no struct_fields
            if type_alias.generic_args.is_empty() {
                // Skip simple type aliases (already handled in Phase 2)
                continue;
            }
            
            // Handle generic type alias - monomorphize it
            let target = &type_alias.target;
            if let Some((_, target_class)) = search_for_class_by_class_name(version_data, target) {
                if let Some(target_data) = version_data.api.values()
                    .find_map(|m| m.classes.get(target_class))
                {
                    generate_monomorphized_type(&mut code, struct_name, type_alias, target_data, version_data);
                }
            }
            continue;
        }

        // Generate struct definition
        if let Some(struct_fields) = &class_data.struct_fields {
            code.push_str(&format!("struct {} {{\r\n", struct_name));

            for field_map in struct_fields {
                for (field_name, field_data) in field_map {
                    let field_type = &field_data.r#type;
                    let ref_kind = &field_data.ref_kind;
                    let array_suffix = field_data.arraysize
                        .map(|n| format!("[{}]", n))
                        .unwrap_or_default();

                    let (c_ptr_prefix, c_ptr_suffix) = ref_kind_to_c_syntax(ref_kind);

                    if is_primitive_arg(field_type) {
                        let c_type = replace_primitive_ctype(field_type);
                        code.push_str(&format!(
                            "    {}{}{} {}{};\r\n",
                            c_ptr_prefix, c_type, c_ptr_suffix, field_name, array_suffix
                        ));
                    } else if let Some((_, type_class_name)) =
                        search_for_class_by_class_name(version_data, field_type)
                    {
                        code.push_str(&format!(
                            "    {}{}{}{} {}{};\r\n",
                            c_ptr_prefix, PREFIX, type_class_name, c_ptr_suffix, field_name, array_suffix
                        ));
                    }
                }
            }

            code.push_str("};\r\n\r\n");
        }
        // Generate enum definition
        else if let Some(enum_fields) = &class_data.enum_fields {
            if !enum_is_union(enum_fields) {
                // Simple enum
                code.push_str(&format!("enum {} {{\r\n", struct_name));
                for variant_map in enum_fields {
                    for (variant_name, _) in variant_map {
                        code.push_str(&format!("   {}_{},\r\n", struct_name, variant_name));
                    }
                }
                code.push_str("};\r\n\r\n");
            } else {
                // Tagged union
                generate_tagged_union(&mut code, struct_name, enum_fields, version_data);
            }
        }
    }

    // Generate macro definitions for enum unions and Vector constructors
    code.push_str("/* MACROS for union enum construction and vector initialization */\r\n\r\n");

    // Generate macros for tagged unions
    for (struct_name, class_data) in &structs {
        if let Some(enum_fields) = &class_data.enum_fields {
            if enum_is_union(enum_fields) {
                for variant_map in enum_fields {
                    for (variant_name, variant_data) in variant_map {
                        if variant_data.r#type.is_some() {
                            code.push_str(&format!(
                                "#define {}_{} (v) {{ .{} = {{ .tag = {}Tag_{}, .payload = v }} \
                                 }}\r\n",
                                struct_name, variant_name, variant_name, struct_name, variant_name
                            ));
                        } else {
                            code.push_str(&format!(
                                "#define {}_{} {{ .{} = {{ .tag = {}Tag_{} }} }}\r\n",
                                struct_name, variant_name, variant_name, struct_name, variant_name
                            ));
                        }
                    }
                }
                code.push_str("\r\n");
            }
        }
    }

    // NOTE: Empty vec constructors removed - they generated invalid C code
    // In C, empty vecs should be created via API functions or initialized manually

    // Generate function declarations
    code.push_str("/* FUNCTIONS */\r\n\r\n");

    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let class_ptr_name = format!("{}{}", PREFIX, class_name);
            let c_is_stack_allocated = class_is_stack_allocated(class_data);
            let class_can_be_copied = class_data
                .derive
                .as_ref()
                .map_or(false, |d| d.contains(&"Copy".to_string()));
            let class_has_recursive_destructor = has_recursive_destructor(version_data, class_data);
            let class_has_custom_destructor = class_data.custom_destructor.unwrap_or(false);
            let treat_external_as_ptr = class_data.external.is_some() && class_data.is_boxed_object;
            // Check if Clone is available (from custom_impls, derive, or deprecated clone field)
            let class_has_clone = class_data
                .custom_impls
                .as_ref()
                .map_or(false, |impls| impls.contains(&"Clone".to_string()))
                || class_data
                    .derive
                    .as_ref()
                    .map_or(false, |d| d.contains(&"Clone".to_string()))
                || class_data.clone.unwrap_or(false);

            // Generate constructors
            if let Some(constructors) = &class_data.constructors {
                for (fn_name, constructor) in constructors {
                    let c_fn_name =
                        format!("{}_{}", class_ptr_name, snake_case_to_lower_camel(fn_name));

                    // Generate function arguments
                    let fn_args = format_c_function_args(
                        api_data,
                        version,
                        constructor,
                        class_name,
                        &class_ptr_name,
                        false, // Constructors don't have self as first arg
                    );

                    // Return type is the class itself
                    let returns = class_ptr_name.clone();

                    code.push_str(&format!(
                        "extern DLLIMPORT {} {}({});\r\n",
                        returns, c_fn_name, fn_args
                    ));
                }
            }

            // Generate methods
            if let Some(functions) = &class_data.functions {
                for (fn_name, function) in functions {
                    let c_fn_name =
                        format!("{}_{}", class_ptr_name, snake_case_to_lower_camel(fn_name));

                    // Generate function arguments
                    let fn_args = format_c_function_args(
                        api_data,
                        version,
                        function,
                        class_name,
                        &class_ptr_name,
                        true, // Methods have self as first arg
                    );

                    // Generate return type
                    let returns = if let Some(return_data) = &function.returns {
                        let (prefix_ptr, base_type, _suffix) = analyze_type(&return_data.r#type);

                        if is_primitive_arg(&base_type) {
                            let c_type = replace_primitive_ctype(&base_type);
                            if prefix_ptr == "*const " || prefix_ptr == "&" {
                                format!("const {}*", c_type)
                            } else if prefix_ptr == "*mut " || prefix_ptr == "&mut " {
                                format!("{}*", c_type)
                            } else {
                                c_type
                            }
                        } else {
                            // Non-primitive type - add PREFIX
                            let c_type = format!("{}{}", PREFIX, base_type);
                            if prefix_ptr == "*const " || prefix_ptr == "&" {
                                format!("const {}*", c_type)
                            } else if prefix_ptr == "*mut " || prefix_ptr == "&mut " {
                                format!("{}*", c_type)
                            } else {
                                c_type
                            }
                        }
                    } else {
                        "void".to_string()
                    };

                    code.push_str(&format!(
                        "extern DLLIMPORT {} {}({});\r\n",
                        returns, c_fn_name, fn_args
                    ));
                }
            }

            // Check if custom Drop is needed
            let class_has_custom_drop = class_data
                .custom_impls
                .as_ref()
                .map_or(false, |impls| impls.contains(&"Drop".to_string()));

            // Generate destructor for types with custom Drop impl or stack-allocated types
            let needs_delete = !class_can_be_copied
                && (class_has_custom_destructor
                    || treat_external_as_ptr
                    || class_has_recursive_destructor
                    || class_has_custom_drop);
            
            if needs_delete {
                code.push_str(&format!(
                    "extern DLLIMPORT void {}_delete({}* restrict instance);\r\n",
                    class_ptr_name, class_ptr_name
                ));
            }

            // Generate deepCopy if the type has Clone impl (regardless of allocation)
            if class_has_clone {
                code.push_str(&format!(
                    "extern DLLIMPORT {} {}_deepCopy({}* const instance);\r\n",
                    class_ptr_name, class_ptr_name, class_ptr_name
                ));
            }

            code.push_str("\r\n");
        }
    }

    // Generate constants
    code.push_str("/* CONSTANTS */\r\n\r\n");

    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            if let Some(constants) = &class_data.constants {
                for constant_map in constants {
                    for (constant_name, constant_data) in constant_map {
                        code.push_str(&format!(
                            "#define {}{}_{} {}\r\n",
                            PREFIX, class_name, constant_name, constant_data.value
                        ));
                    }
                }
            }
        }
    }

    code.push_str("\r\n");

    // Generate helper functions for tagged unions
    code.push_str("/* Union helpers */\r\n\r\n");

    for (struct_name, class_data) in &structs {
        // Skip generic types - they have unresolved type parameters like "T"
        if class_data.generic_params.is_some() {
            continue;
        }
        
        if let Some(enum_fields) = &class_data.enum_fields {
            if enum_is_union(enum_fields) {
                for variant_map in enum_fields {
                    for (variant_name, variant_data) in variant_map {
                        if let Some(variant_type) = &variant_data.r#type {
                            let (_, base_type, _) = analyze_type(variant_type);
                            
                            // Get proper C type for the variant
                            let c_variant_type = if is_primitive_arg(&base_type) {
                                replace_primitive_ctype(&base_type)
                            } else {
                                format!("{}{}", PREFIX, base_type)
                            };

                            // Generate matchRef helper
                            code.push_str(&format!(
                                "bool {}_matchRef{}(const {}* value, const {}** restrict out) \
                                 {{\r\n",
                                struct_name, variant_name, struct_name, c_variant_type
                            ));
                            code.push_str(&format!(
                                "    const {}Variant_{}* casted = (const {}Variant_{}*)value;\r\n",
                                struct_name, variant_name, struct_name, variant_name
                            ));
                            code.push_str(&format!(
                                "    bool valid = casted->tag == {}_Tag_{};\r\n",
                                struct_name, variant_name
                            ));
                            code.push_str(
                                "    if (valid) { *out = &casted->payload; } else { *out = 0; \
                                 }\r\n",
                            );
                            code.push_str("    return valid;\r\n");
                            code.push_str("}\r\n\r\n");

                            // Generate matchMut helper
                            code.push_str(&format!(
                                "bool {}_matchMut{}({}* restrict value, {}* restrict * restrict \
                                 out) {{\r\n",
                                struct_name, variant_name, struct_name, c_variant_type
                            ));
                            code.push_str(&format!(
                                "    {}Variant_{}* restrict casted = ({}Variant_{}* \
                                 restrict)value;\r\n",
                                struct_name, variant_name, struct_name, variant_name
                            ));
                            code.push_str(&format!(
                                "    bool valid = casted->tag == {}_Tag_{};\r\n",
                                struct_name, variant_name
                            ));
                            code.push_str(
                                "    if (valid) { *out = &casted->payload; } else { *out = 0; \
                                 }\r\n",
                            );
                            code.push_str("    return valid;\r\n");
                            code.push_str("}\r\n\r\n");
                        }
                    }
                }
            }
        }
    }

    // Add C patch
    code.push_str("\r\n");
    code.push_str(include_str!("./capi-patch/patch.h"));
    code.push_str("\r\n");

    // End the header file
    code.push_str("\r\n#endif /* AZUL_H */\r\n");

    code
}

/// Collect and sort struct definitions
/// Structs sorted by their dependencies (topological sort)
/// This ensures that types are declared before they are used in C headers
struct SortedStructs<'a> {
    /// Structs in dependency order (types with no dependencies first)
    structs: IndexMap<String, &'a crate::api::ClassData>,
    /// Types that need forward declarations (recursive types like DomVec → Dom)
    forward_declarations: HashMap<String, String>,
}

/// Helper function to check if a type name is a generic type parameter (single uppercase letter)
fn is_generic_type_param(type_name: &str) -> bool {
    type_name.len() == 1 && type_name.chars().next().map(|c| c.is_ascii_uppercase()).unwrap_or(false)
}

/// Get all type dependencies for a class (field types, variant types, type_alias targets)
fn get_type_dependencies(
    class_data: &crate::api::ClassData,
    version_data: &crate::api::VersionData,
) -> Vec<String> {
    let mut deps = Vec::new();
    
    // Struct fields
    if let Some(struct_fields) = &class_data.struct_fields {
        for field_map in struct_fields {
            for (_field_name, field_data) in field_map {
                let (_, base_type, _) = analyze_type(&field_data.r#type);
                if !is_primitive_arg(&base_type) && !is_generic_type_param(&base_type) {
                    deps.push(base_type);
                }
            }
        }
    }
    
    // Enum variants
    if let Some(enum_fields) = &class_data.enum_fields {
        for variant_map in enum_fields {
            for (_variant_name, variant_data) in variant_map {
                if let Some(variant_type) = &variant_data.r#type {
                    let (_, base_type, _) = analyze_type(variant_type);
                    if !is_primitive_arg(&base_type) && !is_generic_type_param(&base_type) {
                        deps.push(base_type);
                    }
                }
            }
        }
    }
    
    // Type alias target and generic args
    if let Some(type_alias) = &class_data.type_alias {
        let target = &type_alias.target;
        if !is_primitive_arg(target) && !is_generic_type_param(target) {
            deps.push(target.clone());
        }
        // Generic args are also dependencies (e.g., CaretColor in CssPropertyValue<CaretColor>)
        for arg in &type_alias.generic_args {
            if !is_primitive_arg(arg) && !is_generic_type_param(arg) {
                deps.push(arg.clone());
            }
        }
    }
    
    deps
}

/// Calculate the dependency depth (chain length) for each type
/// Primitives = 0, types with only primitives = 1, etc.
fn calculate_dependency_depths(
    all_structs: &IndexMap<String, &crate::api::ClassData>,
    version_data: &crate::api::VersionData,
    prefix: &str,
    forward_declarations: &HashMap<String, String>,
) -> HashMap<String, usize> {
    let mut depths: HashMap<String, usize> = HashMap::new();
    let mut changed = true;
    
    // Initialize: callbacks and types with only primitives have depth 0
    for (struct_name, class_data) in all_structs {
        if class_data.callback_typedef.is_some() {
            depths.insert(struct_name.clone(), 0);
            continue;
        }
        
        let deps = get_type_dependencies(class_data, version_data);
        if deps.is_empty() {
            depths.insert(struct_name.clone(), 0);
        }
    }
    
    // Iteratively resolve depths
    let mut iteration = 0;
    while changed && iteration < 500 {
        changed = false;
        iteration += 1;
        
        for (struct_name, class_data) in all_structs {
            if depths.contains_key(struct_name) {
                continue; // Already resolved
            }
            
            let deps = get_type_dependencies(class_data, version_data);
            let mut max_dep_depth: Option<usize> = Some(0);
            
            for dep in &deps {
                // Skip forward-declared recursive types
                if let Some(forward_type) = forward_declarations.get(struct_name) {
                    if dep == forward_type {
                        continue;
                    }
                }
                
                // Find the dependency in all_structs
                let dep_name = format!("{}{}", prefix, dep);
                if let Some(&dep_depth) = depths.get(&dep_name) {
                    max_dep_depth = max_dep_depth.map(|m| m.max(dep_depth));
                } else {
                    // Dependency not yet resolved
                    max_dep_depth = None;
                    break;
                }
            }
            
            if let Some(max_depth) = max_dep_depth {
                depths.insert(struct_name.clone(), max_depth + 1);
                changed = true;
            }
        }
    }
    
    // Any remaining types without depth get max depth (for safety)
    let max_depth = depths.values().copied().max().unwrap_or(0);
    for struct_name in all_structs.keys() {
        depths.entry(struct_name.clone()).or_insert(max_depth + 1);
    }
    
    depths
}

/// Sort structs by their dependencies to avoid forward declarations
/// Returns structs in topological order: types with no dependencies first
fn sort_structs_by_dependencies<'a>(
    api_data: &'a ApiData,
    version: &str,
    prefix: &str,
) -> Result<SortedStructs<'a>> {
    let version_data = api_data.get_version(version).unwrap();

    // Collect all structs first
    let mut all_structs = IndexMap::new();
    for (_module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let struct_name = format!("{}{}", prefix, class_name);
            all_structs.insert(struct_name, class_data);
        }
    }

    // Forward declarations for recursive types
    // These must be manually specified as they create cycles
    let mut forward_declarations = HashMap::new();
    forward_declarations.insert(format!("{}DomVec", prefix), "Dom".to_string());
    forward_declarations.insert(format!("{}MenuItemVec", prefix), "MenuItem".to_string());
    forward_declarations.insert(format!("{}XmlNodeVec", prefix), "XmlNode".to_string());

    // Calculate dependency depths for all types
    let depths = calculate_dependency_depths(&all_structs, version_data, prefix, &forward_declarations);
    
    // Sort structs by depth (ascending)
    let mut structs_with_depths: Vec<_> = all_structs.iter()
        .map(|(name, data)| {
            let depth = depths.get(name).copied().unwrap_or(usize::MAX);
            (name.clone(), *data, depth)
        })
        .collect();
    
    structs_with_depths.sort_by_key(|(_, _, depth)| *depth);
    
    // Build the sorted IndexMap
    let sorted_structs: IndexMap<String, &crate::api::ClassData> = structs_with_depths
        .into_iter()
        .map(|(name, data, _)| (name, data))
        .collect();

    Ok(SortedStructs {
        structs: sorted_structs,
        forward_declarations,
    })
}
