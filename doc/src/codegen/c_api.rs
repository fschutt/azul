use std::collections::HashMap;

use anyhow::{bail, Result};
use indexmap::IndexMap;

use crate::{
    api::ApiData,
    utils::{
        analyze::{
            analyze_type, class_is_stack_allocated, enum_is_union, has_recursive_destructor,
            is_primitive_arg, replace_primitive_ctype, search_for_class_by_class_name,
        },
        string::snake_case_to_lower_camel,
    },
};

const PREFIX: &str = "Az";

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

    // Generate struct definitions
    code.push_str("/* STRUCT DEFINITIONS */\r\n\r\n");

    for (struct_name, class_data) in &structs {
        let is_callback_typedef = class_data.callback_typedef.is_some();

        if is_callback_typedef {
            code.push_str(&format!(
                "typedef /* callback signature */ {};\r\n\r\n",
                struct_name
            ));
            continue;
        }

        if let Some(struct_fields) = &class_data.struct_fields {
            code.push_str(&format!("struct {} {{\r\n", struct_name));

            for field_map in struct_fields {
                for (field_name, field_data) in field_map {
                    let field_type = &field_data.r#type;
                    let (prefix, base_type, suffix) = analyze_type(field_type);

                    if is_primitive_arg(&base_type) {
                        let c_type = replace_primitive_ctype(&base_type);
                        code.push_str(&format!(
                            "    {} {}{} {};\r\n",
                            c_type,
                            replace_primitive_ctype(&prefix),
                            suffix,
                            field_name
                        ));
                    } else if let Some((_, type_class_name)) =
                        search_for_class_by_class_name(version_data, &base_type)
                    {
                        code.push_str(&format!(
                            "    {}{}{}{} {};\r\n",
                            PREFIX,
                            type_class_name,
                            replace_primitive_ctype(&prefix),
                            suffix,
                            field_name
                        ));
                    }
                }
            }

            code.push_str("};\r\n");
            code.push_str(&format!(
                "typedef struct {} {};\r\n\r\n",
                struct_name, struct_name
            ));
        } else if let Some(enum_fields) = &class_data.enum_fields {
            if !enum_is_union(enum_fields) {
                code.push_str(&format!("enum {} {{\r\n", struct_name));

                for variant_map in enum_fields {
                    for (variant_name, _) in variant_map {
                        code.push_str(&format!("   {}_{},\r\n", struct_name, variant_name));
                    }
                }

                code.push_str("};\r\n");
                code.push_str(&format!(
                    "typedef enum {} {};\r\n\r\n",
                    struct_name, struct_name
                ));
            } else {
                // Generate tag enum for tagged union
                code.push_str(&format!("enum {}Tag {{\r\n", struct_name));

                for variant_map in enum_fields {
                    for (variant_name, _) in variant_map {
                        code.push_str(&format!("   {}Tag_{},\r\n", struct_name, variant_name));
                    }
                }

                code.push_str("};\r\n");
                code.push_str(&format!(
                    "typedef enum {}Tag {}Tag;\r\n\r\n",
                    struct_name, struct_name
                ));

                // Generate variant structs for tagged union
                for variant_map in enum_fields {
                    for (variant_name, variant_data) in variant_map {
                        code.push_str(&format!(
                            "struct {}Variant_{} {{ {}Tag tag;",
                            struct_name, variant_name, struct_name
                        ));

                        if let Some(variant_type) = &variant_data.r#type {
                            let (prefix, base_type, suffix) = analyze_type(variant_type);

                            if is_primitive_arg(&base_type) {
                                let c_type = replace_primitive_ctype(&base_type);
                                code.push_str(&format!(
                                    " {}{}{} payload;",
                                    c_type,
                                    replace_primitive_ctype(&prefix),
                                    suffix
                                ));
                            } else if let Some((_, type_class_name)) =
                                search_for_class_by_class_name(version_data, &base_type)
                            {
                                code.push_str(&format!(
                                    " {}{}{}{} payload;",
                                    PREFIX,
                                    type_class_name,
                                    replace_primitive_ctype(&prefix),
                                    suffix
                                ));
                            }
                        }

                        code.push_str(" };\r\n");
                        code.push_str(&format!(
                            "typedef struct {}Variant_{} {}Variant_{};\r\n\r\n",
                            struct_name, variant_name, struct_name, variant_name
                        ));
                    }
                }

                // Generate the union itself
                code.push_str(&format!("union {} {{\r\n", struct_name));

                for variant_map in enum_fields {
                    for (variant_name, _) in variant_map {
                        code.push_str(&format!(
                            "    {}Variant_{} {};\r\n",
                            struct_name, variant_name, variant_name
                        ));
                    }
                }

                code.push_str("};\r\n");
                code.push_str(&format!(
                    "typedef union {} {};\r\n\r\n",
                    struct_name, struct_name
                ));
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

    // Generate "empty" constructor macros for Vec types
    code.push_str("/* Empty vec constructors */\r\n");

    for (module_name, module) in &version_data.api {
        if module_name == "vec" {
            for (class_name, class_data) in &module.classes {
                if class_name.ends_with("Vec") {
                    if let Some(struct_fields) = &class_data.struct_fields {
                        if !struct_fields.is_empty() {
                            if let Some(first_field) = struct_fields.first() {
                                if let Some((field_name, field_data)) = first_field.iter().next() {
                                    if field_name == "ptr" {
                                        let field_type = &field_data.r#type;
                                        let (_, base_type, _) = analyze_type(field_type);

                                        if is_primitive_arg(&base_type) {
                                            let c_type = replace_primitive_ctype(&base_type);
                                            code.push_str(&format!(
                                                "{} {}Array[] = {};\r\n",
                                                c_type, PREFIX, class_name
                                            ));
                                            code.push_str(&format!(
                                                "#define {}_{} {{ .ptr = &{}Array, .len = 0, .cap \
                                                 = 0 }}\r\n",
                                                class_name, "empty", class_name
                                            ));
                                        } else {
                                            code.push_str(&format!(
                                                "{}{} {}Array[] = {};\r\n",
                                                PREFIX, base_type, PREFIX, class_name
                                            ));
                                            code.push_str(&format!(
                                                "#define {}_{} {{ .ptr = &{}Array, .len = 0, .cap \
                                                 = 0 }}\r\n",
                                                class_name, "empty", class_name
                                            ));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    code.push_str("\r\n");

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
            let class_can_be_cloned = class_data.clone.unwrap_or(true);

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

            // Generate destructor and deep copy methods
            if c_is_stack_allocated {
                if !class_can_be_copied
                    && (class_has_custom_destructor
                        || treat_external_as_ptr
                        || class_has_recursive_destructor)
                {
                    code.push_str(&format!(
                        "extern DLLIMPORT void {}_delete({}* restrict instance);\r\n",
                        class_ptr_name, class_ptr_name
                    ));
                }

                if treat_external_as_ptr && class_can_be_cloned {
                    code.push_str(&format!(
                        "extern DLLIMPORT {} {}_deepCopy({}* const instance);\r\n",
                        class_ptr_name, class_ptr_name, class_ptr_name
                    ));
                }
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
        if let Some(enum_fields) = &class_data.enum_fields {
            if enum_is_union(enum_fields) {
                for variant_map in enum_fields {
                    for (variant_name, variant_data) in variant_map {
                        if let Some(variant_type) = &variant_data.r#type {
                            let (_, base_type, _) = analyze_type(variant_type);

                            // Generate matchRef helper
                            code.push_str(&format!(
                                "bool {}_matchRef{}(const {}* value, const {}{}** restrict out) \
                                 {{\r\n",
                                struct_name, variant_name, struct_name, PREFIX, base_type
                            ));
                            code.push_str(&format!(
                                "    const {}Variant_{}* casted = (const {}Variant_{}*)value;\r\n",
                                struct_name, variant_name, struct_name, variant_name
                            ));
                            code.push_str(&format!(
                                "    bool valid = casted->tag == {}Tag_{};\r\n",
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
                                "bool {}_matchMut{}({}* restrict value, {}{}* restrict * restrict \
                                 out) {{\r\n",
                                struct_name, variant_name, struct_name, PREFIX, base_type
                            ));
                            code.push_str(&format!(
                                "    {}Variant_{}* restrict casted = ({}Variant_{}* \
                                 restrict)value;\r\n",
                                struct_name, variant_name, struct_name, variant_name
                            ));
                            code.push_str(&format!(
                                "    bool valid = casted->tag == {}Tag_{};\r\n",
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
    for (module_name, module) in &version_data.api {
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

    let mut sorted_structs = IndexMap::new();
    let mut remaining_structs = all_structs.clone();

    // First pass: Insert all types that only have primitive types as fields or are callbacks
    let mut to_remove = Vec::new();
    for (struct_name, class_data) in &remaining_structs {
        let is_callback = class_data.callback_typedef.is_some();
        let has_forward_decl = forward_declarations.contains_key(struct_name);

        let mut has_only_primitives = true;

        if !is_callback {
            // Check struct fields
            if let Some(struct_fields) = &class_data.struct_fields {
                for field_map in struct_fields {
                    for (field_name, field_data) in field_map {
                        let (_, base_type, _) = analyze_type(&field_data.r#type);

                        if !is_primitive_arg(&base_type) {
                            // Check if this field references the forward-declared type
                            if let Some(forward_type) = forward_declarations.get(struct_name) {
                                if &base_type == forward_type {
                                    continue; // Skip forward-declared recursive reference
                                }
                            }

                            // Check if field is a function pointer (typedef)
                            if let Some((mod_name, class_name)) =
                                search_for_class_by_class_name(version_data, &base_type)
                            {
                                if let Some(field_class) = version_data
                                    .api
                                    .get(mod_name)
                                    .and_then(|m| m.classes.get(class_name))
                                {
                                    if field_class.callback_typedef.is_some() {
                                        continue; // Function pointers are OK
                                    }
                                }
                            }

                            has_only_primitives = false;
                            break;
                        }
                    }
                    if !has_only_primitives {
                        break;
                    }
                }
            }
            // Check enum variants
            else if let Some(enum_fields) = &class_data.enum_fields {
                for variant_map in enum_fields {
                    for (variant_name, variant_data) in variant_map {
                        if let Some(variant_type) = &variant_data.r#type {
                            let (_, base_type, _) = analyze_type(variant_type);

                            if !is_primitive_arg(&base_type) {
                                // Check forward declaration
                                if let Some(forward_type) = forward_declarations.get(struct_name) {
                                    if &base_type == forward_type {
                                        continue;
                                    }
                                }

                                // Check if it's a function pointer
                                if let Some((mod_name, class_name)) =
                                    search_for_class_by_class_name(version_data, &base_type)
                                {
                                    if let Some(field_class) = version_data
                                        .api
                                        .get(mod_name)
                                        .and_then(|m| m.classes.get(class_name))
                                    {
                                        if field_class.callback_typedef.is_some() {
                                            continue;
                                        }
                                    }
                                }

                                has_only_primitives = false;
                                break;
                            }
                        }
                    }
                    if !has_only_primitives {
                        break;
                    }
                }
            }
        }

        if is_callback || has_only_primitives {
            to_remove.push(struct_name.clone());
        }
    }

    // Move primitives to sorted list
    for name in &to_remove {
        if let Some(data) = remaining_structs.shift_remove(name) {
            sorted_structs.insert(name.clone(), data);
        }
    }

    // Iterative resolution: Keep adding structs whose dependencies are already resolved
    let mut iteration_count = 0;
    while !remaining_structs.is_empty() {
        let mut resolved_this_iteration = Vec::new();

        for (struct_name, class_data) in &remaining_structs {
            let mut all_deps_resolved = true;

            // Check struct fields
            if let Some(struct_fields) = &class_data.struct_fields {
                for field_map in struct_fields {
                    for (field_name, field_data) in field_map {
                        let (_, base_type, _) = analyze_type(&field_data.r#type);

                        if !is_primitive_arg(&base_type) {
                            // Check forward declaration
                            if let Some(forward_type) = forward_declarations.get(struct_name) {
                                if &base_type == forward_type {
                                    continue;
                                }
                            }

                            // Check if it's a callback
                            if let Some((mod_name, class_name)) =
                                search_for_class_by_class_name(version_data, &base_type)
                            {
                                if let Some(field_class) = version_data
                                    .api
                                    .get(mod_name)
                                    .and_then(|m| m.classes.get(class_name))
                                {
                                    if field_class.callback_typedef.is_some() {
                                        continue;
                                    }
                                }

                                // Check if dependency is already resolved
                                let dep_name = format!("{}{}", prefix, class_name);
                                if !sorted_structs.contains_key(&dep_name) {
                                    all_deps_resolved = false;
                                    break;
                                }
                            } else {
                                // Type not found in API - this is an error unless it's a base type
                                bail!(
                                    "Type '{}' not found in API (referenced by field '{}' in \
                                     struct '{}'). All non-primitive types must be defined in the \
                                     API.",
                                    base_type,
                                    field_name,
                                    struct_name
                                );
                            }
                        }
                    }
                    if !all_deps_resolved {
                        break;
                    }
                }
            }
            // Check enum variants
            else if let Some(enum_fields) = &class_data.enum_fields {
                for variant_map in enum_fields {
                    for (variant_name, variant_data) in variant_map {
                        if let Some(variant_type) = &variant_data.r#type {
                            let (_, base_type, _) = analyze_type(variant_type);

                            if !is_primitive_arg(&base_type) {
                                // Check forward declaration
                                if let Some(forward_type) = forward_declarations.get(struct_name) {
                                    if &base_type == forward_type {
                                        continue;
                                    }
                                }

                                // Check if it's a callback
                                if let Some((mod_name, class_name)) =
                                    search_for_class_by_class_name(version_data, &base_type)
                                {
                                    if let Some(field_class) = version_data
                                        .api
                                        .get(mod_name)
                                        .and_then(|m| m.classes.get(class_name))
                                    {
                                        if field_class.callback_typedef.is_some() {
                                            continue;
                                        }
                                    }

                                    // Check if dependency is resolved
                                    let dep_name = format!("{}{}", prefix, class_name);
                                    if !sorted_structs.contains_key(&dep_name) {
                                        all_deps_resolved = false;
                                        break;
                                    }
                                } else {
                                    bail!(
                                        "Type '{}' not found in API (referenced by enum variant \
                                         '{}' in enum '{}'). All non-primitive types must be \
                                         defined in the API.",
                                        base_type,
                                        variant_name,
                                        struct_name
                                    );
                                }
                            }
                        }
                    }
                    if !all_deps_resolved {
                        break;
                    }
                }
            }

            if all_deps_resolved {
                resolved_this_iteration.push(struct_name.clone());
            }
        }

        // Check if we made progress
        if resolved_this_iteration.is_empty() && !remaining_structs.is_empty() {
            let unresolved: Vec<String> = remaining_structs.keys().cloned().collect();
            bail!(
                "Circular dependency detected! Unable to resolve types: {:?}\nConsider adding \
                 these types to forward_declarations if they are recursive.",
                unresolved
            );
        }

        // Move resolved structs to sorted list
        for name in &resolved_this_iteration {
            if let Some(data) = remaining_structs.shift_remove(name) {
                sorted_structs.insert(name.clone(), data);
            }
        }

        iteration_count += 1;
        if iteration_count > 500 {
            bail!(
                "Infinite loop detected in struct sorting (>500 iterations). This indicates a bug \
                 in the sorting algorithm."
            );
        }
    }

    Ok(SortedStructs {
        structs: sorted_structs,
        forward_declarations,
    })
}
