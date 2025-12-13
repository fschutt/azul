use std::collections::HashMap;

use crate::api::{ClassData, ModuleData, VersionData};

// Basic Rust types that are treated as primitives
const BASIC_TYPES: [&str; 19] = [
    "bool", "f32", "f64", "fn", "i128", "i16", "i32", "i64", "i8", "isize", "slice", "u128", "u16",
    "u32", "u64", "u8", "()", "usize", "c_void",
];

/// Check if an argument type is a primitive type
pub fn is_primitive_arg(arg: &str) -> bool {
    BASIC_TYPES.contains(&get_stripped_arg(arg))
}

/// Strip reference/pointer specifiers from a type
pub fn get_stripped_arg(arg: &str) -> &str {
    let arg = arg.trim();
    let arg = arg.strip_prefix("&").unwrap_or(arg);
    let arg = arg.strip_prefix("&mut").unwrap_or(arg);
    let arg = arg.strip_prefix("*const").unwrap_or(arg);
    let arg = arg.strip_prefix("*mut").unwrap_or(arg);
    arg.trim()
}

/// Analyze a type string and return its components
///
/// Returns (prefix, base_type, suffix)
/// Example: "&mut Vec<T>" returns ("&mut ", "Vec", "<T>")
pub fn analyze_type(arg_type: &str) -> (String, String, String) {
    // Determine the prefix (reference/pointer type)
    let mut starts = String::new();
    let mut arg_type_clean = arg_type.to_string();

    if arg_type.starts_with("&mut") {
        starts = "&mut ".to_string();
        arg_type_clean = arg_type.replace("&mut", "");
    } else if arg_type.starts_with('&') {
        starts = "&".to_string();
        arg_type_clean = arg_type.replace('&', "");
    } else if arg_type.starts_with("* const") {
        // Handle "* const" with space (from syn/api.json)
        starts = "* const ".to_string();
        arg_type_clean = arg_type.replace("* const", "");
    } else if arg_type.starts_with("*const") {
        starts = "*const ".to_string();
        arg_type_clean = arg_type.replace("*const", "");
    } else if arg_type.starts_with("* mut") {
        // Handle "* mut" with space (from syn/api.json)
        starts = "* mut ".to_string();
        arg_type_clean = arg_type.replace("* mut", "");
    } else if arg_type.starts_with("*mut") {
        starts = "*mut ".to_string();
        arg_type_clean = arg_type.replace("*mut", "");
    }

    arg_type_clean = arg_type_clean.trim().to_string();

    // Handle array types: [T; N]
    let mut ends = String::new();
    if arg_type_clean.starts_with('[') && arg_type_clean.ends_with(']') {
        let arg_type_parts: Vec<String> = arg_type_clean[1..arg_type_clean.len() - 1]
            .split(';')
            .map(|s| s.to_string())
            .collect();
        if arg_type_parts.len() > 1 {
            arg_type_clean = arg_type_parts[0].trim().to_string();
            starts.push('['); // Add opening bracket to prefix
            ends = format!("; {}]", arg_type_parts[1].trim());
        }
    }

    (starts, arg_type_clean, ends)
}

/// Search for imports of a specific argument type
pub fn search_imports_arg_type(
    class_data: &ClassData,
    search_type: &str,
    arg_types_to_search: &mut Vec<String>,
) {
    if let Some(constructors) = class_data.constructors.as_ref() {
        if constructors.contains_key(search_type) {
            if let Some(function) = constructors.get(search_type) {
                for arg_object in &function.fn_args {
                    for (arg_name, arg_type) in arg_object.iter() {
                        if arg_name != "self" {
                            arg_types_to_search.push(arg_type.clone());
                        }
                    }
                }
            }
        }
    }

    if let Some(functions) = class_data.functions.as_ref() {
        if functions.contains_key(search_type) {
            if let Some(function) = functions.get(search_type) {
                for arg_object in &function.fn_args {
                    for (arg_name, arg_type) in arg_object.iter() {
                        if arg_name != "self" {
                            arg_types_to_search.push(arg_type.clone());
                        }
                    }
                }
            }
        }
    }
}

/// Get all imports needed for a module
pub fn get_all_imports(
    version_data: &VersionData,
    module: &ModuleData,
    module_name: &str,
) -> String {
    let mut imports = HashMap::new();
    let mut arg_types_to_search = Vec::new();

    // Collect all types from function arguments
    for (class_name, class_data) in &module.classes {
        search_imports_arg_type(class_data, "constructors", &mut arg_types_to_search);
        search_imports_arg_type(class_data, "functions", &mut arg_types_to_search);
    }

    // Find where each type is defined
    for arg in arg_types_to_search.iter() {
        let arg = arg
            .replace("*const", "")
            .replace("*mut", "")
            .trim()
            .to_string();

        if is_primitive_arg(&arg) {
            continue;
        }

        if let Some((found_module, found_class)) =
            search_for_class_by_class_name(version_data, &arg)
        {
            if found_module != module_name {
                imports
                    .entry(found_module.to_string())
                    .or_insert_with(Vec::new)
                    .push(found_class.to_string());
            }
        } else {
            eprintln!("Type not found: {}", arg);
        }
    }

    // Generate the import statements
    let mut imports_str = String::new();
    for (module_name, classes) in imports {
        if classes.len() == 1 {
            imports_str.push_str(&format!(
                "    use crate::{}::{};\r\n",
                module_name, classes[0]
            ));
        } else {
            let mut class_list = classes.clone();
            class_list.sort();

            imports_str.push_str(&format!("    use crate::{}::{{", module_name));
            for (i, c) in class_list.iter().enumerate() {
                if i > 0 {
                    imports_str.push_str(", ");
                }
                imports_str.push_str(c);
            }
            imports_str.push_str("};\r\n");
        }
    }

    imports_str
}

/// Search for a class by name in all modules
pub fn search_for_class_by_class_name<'a>(
    version_data: &'a VersionData,
    searched_class_name: &str,
) -> Option<(&'a str, &'a str)> {
    // Get the latest version
    for (module_name, module) in &version_data.api {
        for (class_name, _) in &module.classes {
            if class_name == searched_class_name {
                return Some((module_name, class_name));
            }
        }
    }

    None
}

/// Get a class by module name and class name
pub fn get_class<'a>(
    version_data: &'a VersionData,
    module_name: &str,
    class_name: &str,
) -> Option<&'a ClassData> {
    version_data.api.get(module_name)?.classes.get(class_name)
}

/// Check if a class is stack allocated
///
/// A class is stack-allocated when:
/// - It has `struct_fields` or `enum_fields` (has actual data layout)
/// - AND it has `repr: C` (has stable C-compatible layout)
/// - AND `is_boxed_object` is NOT true (not explicitly marked as pointer-wrapper)
pub fn class_is_stack_allocated(class: &ClassData) -> bool {
    // If explicitly marked as boxed object, it's not stack allocated
    if class.is_boxed_object {
        return false;
    }

    // Callback typedefs are function pointers, always stack-allocated
    if class.callback_typedef.is_some() {
        return true;
    }

    // Const values are stack-allocated
    if class.const_value_type.is_some() {
        return true;
    }

    // Type aliases without struct/enum fields are stack-allocated
    // (they just alias another type)
    if class.type_alias.is_some() {
        return true;
    }

    // Structs and enums with repr(C) are stack-allocated
    let has_repr_c = class.repr.as_deref() == Some("C");
    let has_data_layout = class.struct_fields.is_some() || class.enum_fields.is_some();

    has_data_layout && has_repr_c
}

/// Check if a class is a small enum (only has enum fields)
pub fn class_is_small_enum(class: &ClassData) -> bool {
    class.enum_fields.is_some()
}

/// Check if a class is a small struct (only has struct fields)
pub fn class_is_small_struct(class: &ClassData) -> bool {
    class.struct_fields.is_some()
}

/// Check if a class is a typedef
pub fn class_is_typedef(class: &ClassData) -> bool {
    class.callback_typedef.is_some()
}

/// Check if a class has a recursive destructor
pub fn has_recursive_destructor(version_data: &VersionData, class: &ClassData) -> bool {
    // Simple typedef has no destructor
    if class_is_typedef(class) {
        return false;
    }

    // Explicit custom destructor or external boxed object
    let has_custom_destructor = class.custom_destructor.unwrap_or(false);
    let is_boxed_object = class.is_boxed_object;
    let treat_external_as_ptr = class.external.is_some() && is_boxed_object;

    if has_custom_destructor || treat_external_as_ptr {
        return true;
    }

    // Check struct fields for types with destructors
    if let Some(struct_fields) = &class.struct_fields {
        for field_map in struct_fields {
            for (field_name, field_data) in field_map {
                let field_type = &field_data.r#type;
                let (_, field_type_base, _) = analyze_type(field_type);

                if !is_primitive_arg(&field_type_base) {
                    if let Some((module_name, class_name)) =
                        search_for_class_by_class_name(version_data, &field_type_base)
                    {
                        if let Some(field_class) = get_class(version_data, module_name, class_name)
                        {
                            if has_recursive_destructor(version_data, field_class) {
                                return true;
                            }
                        }
                    }
                }
            }
        }
    }

    // Check enum variants for types with destructors
    if let Some(enum_fields) = &class.enum_fields {
        for variant_map in enum_fields {
            for (_, variant_data) in variant_map {
                if let Some(variant_type) = &variant_data.r#type {
                    let (_, variant_type_base, _) = analyze_type(variant_type);

                    if !is_primitive_arg(&variant_type_base) {
                        if let Some((module_name, class_name)) =
                            search_for_class_by_class_name(version_data, &variant_type_base)
                        {
                            if let Some(variant_class) =
                                get_class(version_data, module_name, class_name)
                            {
                                if has_recursive_destructor(version_data, variant_class) {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    false
}

/// Replace primitive Rust type with C type
pub fn replace_primitive_ctype(input: &str) -> String {
    let input = input.trim();
    match input {
        "*const" => "* ",
        "*mut" => "* restrict ",
        "i8" => "int8_t",
        "u8" => "uint8_t",
        "i16" => "int16_t",
        "u16" => "uint16_t",
        "i32" => "int32_t",
        "i64" => "int64_t",
        "isize" => "ssize_t",
        "u32" => "uint32_t",
        "u64" => "uint64_t",
        "f32" => "float",
        "f64" => "double",
        "usize" => "size_t",
        "c_void" => "void",
        _ => input,
    }
    .to_string()
}

/// Check if an enum type is a tagged union
pub fn enum_is_union(
    enum_fields: &Vec<indexmap::IndexMap<String, crate::api::EnumVariantData>>,
) -> bool {
    for variant in enum_fields {
        for (_, variant_data) in variant {
            if variant_data.r#type.is_some() {
                return true;
            }
        }
    }
    false
}
