use std::collections::HashMap;

use crate::{
    api::ApiData,
    codegen::{
        func_gen::{build_functions_map, generate_rust_dll_bindings},
        struct_gen::{GenerateConfig, StructMetadata},
    },
    utils::{
        analyze::{
            analyze_type, class_is_stack_allocated, get_all_imports, is_primitive_arg,
            search_for_class_by_class_name,
        },
        string::snake_case_to_lower_camel,
    },
};

// Patches that will be included in the Rust API
static RUST_API_PATCHES: &[(&str, &str)] = &[
    ("str", include_str!("./api-patch/string.rs")),
    ("vec", include_str!("./api-patch/vec.rs")),
    ("option", include_str!("./api-patch/option.rs")),
    ("dom", include_str!("./api-patch/dom.rs")),
    ("gl", include_str!("./api-patch/gl.rs")),
    ("css", include_str!("./api-patch/css.rs")),
    ("window", include_str!("./api-patch/window.rs")),
    ("callbacks", include_str!("./api-patch/callbacks.rs")),
];

/// Primitive types that should never get an Az prefix
const PRIMITIVE_TYPES: &[&str] = &[
    "bool", "f32", "f64", "fn", "i128", "i16", "i32", "i64", "i8", "isize", 
    "slice", "u128", "u16", "u32", "u64", "u8", "usize", "c_void",
    "str", "char", "c_char", "c_schar", "c_uchar",
];

/// Single-letter types are usually generic type parameters
fn is_generic_type_param(type_name: &str) -> bool {
    type_name.len() == 1 && type_name.chars().next().map(|c| c.is_ascii_uppercase()).unwrap_or(false)
}

/// Generate Rust API code from API data
pub fn generate_rust_api(api_data: &ApiData, version: &str) -> String {
    let mut module_file_map = HashMap::new();

    // Get the latest version
    let version_data = api_data.get_version(version).unwrap();

    // Compute version-based prefix
    let prefix = api_data
        .get_version_prefix(version)
        .unwrap_or_else(|| "Az".to_string());

    // Build structs map for DLL generation
    let mut structs_map = std::collections::HashMap::new();
    for (module_name, module_data) in &version_data.api {
        for (class_name, class_data) in &module_data.classes {
            // Skip primitive types and generic type parameters
            if PRIMITIVE_TYPES.contains(&class_name.as_str()) || is_generic_type_param(class_name) {
                continue;
            }
            let metadata = StructMetadata::from_class_data(class_name.clone(), class_data);
            // Always add prefix, even if type already has it (consistency)
            let prefixed_name = format!("{}{}", prefix, class_name);
            structs_map.insert(prefixed_name, metadata);
        }
    }

    // Build functions map
    let functions_map = build_functions_map(version_data, &prefix).unwrap();

    // Generate Rust DLL bindings
    let dll_config = GenerateConfig {
        prefix: prefix.clone(),
        indent: 8,
        private_pointers: false,
        no_derive: false,
        wrapper_postfix: String::new(),
        ..Default::default()
    };

    let dll_code =
        generate_rust_dll_bindings(version_data, &structs_map, &functions_map, &dll_config)
            .unwrap_or_else(|e| format!("// Error generating DLL bindings: {}\n", e));

    module_file_map.insert("dll".to_string(), dll_code);

    // Process all modules
    for (module_name, module) in &version_data.api {
        let mut code = String::new();

        code.push_str("    #![allow(dead_code, unused_imports, unused_unsafe)]\r\n");

        // Add module documentation
        if let Some(doc) = &module.doc {
            for line in doc {
                code.push_str(&format!("    //! {}\r\n", line));
            }
        }

        code.push_str("    use crate::dll::*;\r\n");
        code.push_str("    use core::ffi::c_void;\r\n");

        // Add patches if available
        for (patch_modules, patch_content) in RUST_API_PATCHES {
            if patch_modules.contains(&module_name.as_str()) {
                code.push_str(patch_content);
                code.push_str("\r\n");
            }
        }

        // Add imports
        code.push_str(&get_all_imports(version_data, module, module_name));

        // Process all classes in this module
        for (class_name, class_data) in &module.classes {
            // Class properties
            let class_can_derive_debug = class_data
                .derive
                .as_ref()
                .map_or(false, |d| d.contains(&"Debug".to_string()));
            let class_can_be_copied = class_data
                .derive
                .as_ref()
                .map_or(false, |d| d.contains(&"Copy".to_string()));
            let class_has_partialeq = class_data
                .derive
                .as_ref()
                .map_or(false, |d| d.contains(&"PartialEq".to_string()));
            let class_has_eq = class_data
                .derive
                .as_ref()
                .map_or(false, |d| d.contains(&"Eq".to_string()));
            let class_has_partialord = class_data
                .derive
                .as_ref()
                .map_or(false, |d| d.contains(&"PartialOrd".to_string()));
            let class_has_ord = class_data
                .derive
                .as_ref()
                .map_or(false, |d| d.contains(&"Ord".to_string()));
            let class_can_be_hashed = class_data
                .derive
                .as_ref()
                .map_or(false, |d| d.contains(&"Hash".to_string()));

            let class_is_boxed_object = !class_is_stack_allocated(class_data);
            let class_is_const = class_data.const_value_type.is_some();
            let class_is_callback_typedef = class_data.callback_typedef.is_some();
            let class_has_custom_destructor = class_data.custom_destructor.unwrap_or(false);
            let treat_external_as_ptr = class_data.external.is_some() && class_data.is_boxed_object;

            let class_can_be_cloned = class_data.clone.unwrap_or(true);

            let c_is_stack_allocated = !class_is_boxed_object;
            // Always add prefix, even if type already has it (consistency)
            let class_ptr_name = format!("{}{}", prefix, class_name);

            code.push_str("\r\n");

            // Add class documentation
            if let Some(doc) = &class_data.doc {
                for line in doc {
                    code.push_str(&format!("    /// {}\r\n    ", line));
                }
            } else {
                code.push_str(&format!("    /// `{}` struct\r\n    ", class_name));
            }

            code.push_str(&format!(
                "\r\n    #[doc(inline)] pub use crate::dll::{} as {};\r\n",
                class_ptr_name, class_name
            ));

            let has_constructors = class_data
                .constructors
                .as_ref()
                .map_or(false, |c| !c.is_empty());
            let has_functions = class_data
                .functions
                .as_ref()
                .map_or(false, |f| !f.is_empty());
            let has_constants = class_data
                .constants
                .as_ref()
                .map_or(false, |c| !c.is_empty());

            let should_emit_impl = has_constructors
                || has_functions
                || has_constants && !(class_is_const || class_is_callback_typedef);

            if should_emit_impl {
                let mut class_impl_block = String::from("\r\n");

                // Add constants
                if let Some(constants) = &class_data.constants {
                    for constant_map in constants {
                        for (constant_name, constant_data) in constant_map {
                            let constant_type = &constant_data.r#type;
                            let constant_value = &constant_data.value;
                            class_impl_block.push_str(&format!(
                                "        pub const {}: {} = {};\r\n",
                                constant_name, constant_type, constant_value
                            ));
                        }
                    }

                    class_impl_block.push_str("\r\n");
                }

                // Add constructors
                if let Some(constructors) = &class_data.constructors {
                    for (fn_name, constructor) in constructors {
                        let c_fn_name =
                            format!("{}_{}", class_ptr_name, snake_case_to_lower_camel(fn_name));

                        // Placeholder for real implementation
                        let fn_args = "/* args would go here */";
                        let fn_args_call = "/* args call would go here */";

                        let mut fn_body = String::new();

                        // Check if there's a custom patch for this function
                        if false
                        /* check for patch here */
                        {
                            fn_body = "/* patched function body */".to_string();
                        } else {
                            fn_body = format!(
                                "unsafe {{ crate::dll::{} ({}) }}",
                                c_fn_name, fn_args_call
                            );
                        }

                        // Add constructor documentation
                        if let Some(doc) = &constructor.doc {
                            for line in doc {
                                class_impl_block.push_str(&format!("        /// {}\r\n", line));
                            }
                        } else {
                            class_impl_block.push_str(&format!(
                                "        /// Creates a new `{}` instance.\r\n",
                                class_name
                            ));
                        }

                        // Determine return type
                        let mut returns = "Self".to_string();
                        if let Some(return_info) = &constructor.returns {
                            let return_type = &return_info.r#type;
                            let (prefix, type_name, suffix) = analyze_type(return_type);

                            if is_primitive_arg(&type_name) {
                                returns = return_type.clone();
                            } else if let Some((return_module, return_class)) =
                                search_for_class_by_class_name(version_data, &type_name)
                            {
                                returns = format!(
                                    "{} crate::{}::{}{}",
                                    prefix, return_module, return_class, suffix
                                );
                            }
                        }

                        // Add constructor method
                        class_impl_block.push_str(&format!(
                            "        pub fn {}({}) -> {} {{ {} }}\r\n",
                            fn_name, fn_args, returns, fn_body
                        ));
                    }
                }

                // Add methods
                if let Some(functions) = &class_data.functions {
                    for (fn_name, function) in functions {
                        let c_fn_name =
                            format!("{}_{}", class_ptr_name, snake_case_to_lower_camel(fn_name));

                        // Placeholder for real implementation
                        let fn_args = "/* args would go here */";
                        let fn_args_call = "/* args call would go here */";

                        let mut fn_body = String::new();

                        // Check if there's a custom patch for this function
                        if false
                        /* check for patch here */
                        {
                            class_impl_block.push_str("/* patched function */");
                            continue;
                        }

                        fn_body =
                            format!("unsafe {{ crate::dll::{} ({}) }}", c_fn_name, fn_args_call);

                        // Add method documentation
                        if let Some(doc) = &function.doc {
                            for line in doc {
                                class_impl_block.push_str(&format!("        /// {}\r\n", line));
                            }
                        } else {
                            class_impl_block.push_str(&format!(
                                "        /// Calls the `{}::{}` function.\r\n",
                                class_name, fn_name
                            ));
                        }

                        // Determine return type
                        let mut returns = String::new();
                        if let Some(return_info) = &function.returns {
                            let return_type = &return_info.r#type;
                            let (prefix, type_name, suffix) = analyze_type(return_type);

                            if is_primitive_arg(&type_name) {
                                returns = format!(" -> {}", return_type);
                            } else if let Some((return_module, return_class)) =
                                search_for_class_by_class_name(version_data, &type_name)
                            {
                                returns = format!(
                                    " ->{} crate::{}::{}{}",
                                    prefix, return_module, return_class, suffix
                                );
                            }
                        }

                        // Add method
                        class_impl_block.push_str(&format!(
                            "        pub fn {}({}){} {{ {} }}\r\n",
                            fn_name, fn_args, returns, fn_body
                        ));
                    }
                }

                code.push_str(&format!("    impl {} {{\r\n", class_name));
                code.push_str(&class_impl_block);
                code.push_str("    }\r\n\r\n"); // end of class impl
            }

            // Add Clone implementation if needed
            if treat_external_as_ptr && class_can_be_cloned {
                code.push_str(&format!(
                    "    impl Clone for {} {{ fn clone(&self) -> Self {{ unsafe {{ \
                     crate::dll::{}_deepCopy(self) }} }} }}\r\n",
                    class_name, class_ptr_name
                ));
            }

            // Add Drop implementation if needed
            if treat_external_as_ptr {
                code.push_str(&format!(
                    "    impl Drop for {} {{ fn drop(&mut self) {{ if self.run_destructor {{ \
                     unsafe {{ crate::dll::{}_delete(self) }} }} }} }}\r\n",
                    class_name, class_ptr_name
                ));
            }
        }

        module_file_map.insert(module_name.to_string(), code);
    }

    // Combine all modules into final code
    let mut final_code = String::new();

    // Add license header - in a real implementation you'd read this from a file
    final_code.push_str("// LICENSE header would be included here\r\n\r\n");

    // Add Rust header - in a real implementation you'd read this from a file
    final_code.push_str(include_str!("./api-patch/header.rs"));

    // Add all modules
    for module_name in module_file_map.keys() {
        if module_name != "dll" {
            final_code.push_str("pub ");
        }

        final_code.push_str(&format!("mod {} {{\r\n", module_name));
        final_code.push_str(&module_file_map[module_name]);
        final_code.push_str("}\r\n\r\n");
    }

    final_code
}
