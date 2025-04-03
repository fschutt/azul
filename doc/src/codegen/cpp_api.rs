use crate::api::ApiData;
use crate::utils::string::snake_case_to_lower_camel;
use crate::utils::analyze::{
    is_primitive_arg, analyze_type, search_for_class_by_class_name, 
    get_class, class_is_stack_allocated, has_recursive_destructor,
    class_is_small_enum, class_is_small_struct, class_is_typedef,
    enum_is_union, replace_primitive_ctype
};
use std::collections::{BTreeMap, HashMap};
use indexmap::IndexMap;

const PREFIX: &str = "Az";

/// Generate C++ API code from API data
pub fn generate_cpp_api(api_data: &ApiData) -> String {
    let mut code = String::new();
    
    // Get the latest version
    let latest_version = api_data.get_latest_version_str().unwrap();
    let version_data = api_data.get_version(latest_version).unwrap();
    
    // Start C++ header file
    code.push_str("#ifndef AZUL_H\r\n");
    code.push_str("#define AZUL_H\r\n");
    code.push_str("\r\n");
    code.push_str("namespace dll {\r\n");
    code.push_str("\r\n");
    code.push_str("    #include <cstdint>\r\n"); // uint8_t, ...
    code.push_str("    #include <cstddef>\r\n"); // size_t
    
    // Collect all structs to be sorted later
    let structs = collect_structs(api_data);
    
    // Generate struct definitions - simplified for brevity
    code.push_str("    /* STRUCT DEFINITIONS */\r\n\r\n");
    
    for (struct_name, class_data) in &structs {
        let is_callback_typedef = class_data.callback_typedef.is_some();
        
        if is_callback_typedef {
            code.push_str(&format!("    using {} = /* callback signature */;\r\n\r\n", struct_name));
            continue;
        }
        
        if let Some(struct_fields) = &class_data.struct_fields {
            code.push_str(&format!("    struct {} {{\r\n", struct_name));
            
            for field_map in struct_fields {
                for (field_name, field_data) in field_map {
                    let field_type = &field_data.r#type;
                    let (prefix, base_type, suffix) = analyze_type(field_type);
                    
                    if is_primitive_arg(&base_type) {
                        let c_type = replace_primitive_ctype(&base_type);
                        code.push_str(&format!("        {} {}{} {};\r\n", 
                                             c_type,
                                             replace_primitive_ctype(&prefix),
                                             suffix,
                                             field_name));
                    } else if let Some((_, type_class_name)) = search_for_class_by_class_name(api_data, &base_type) {
                        code.push_str(&format!("        {}{}{} {};\r\n", 
                                             type_class_name,
                                             replace_primitive_ctype(&prefix),
                                             suffix,
                                             field_name));
                    }
                }
            }
            
            // Add C++ specific methods
            code.push_str("        // C++ specific methods\r\n");
            code.push_str(&format!("        {}& operator=(const {}&) = delete; /* disable assignment operator, use std::move (default) or .clone() */\r\n", struct_name, struct_name));
            
            let class_can_be_copied = class_data.derive.as_ref().map_or(false, |d| d.contains(&"Copy".to_string()));
            if !class_can_be_copied {
                code.push_str(&format!("        {}(const {}&) = delete; /* disable copy constructor, use explicit .clone() */\r\n", struct_name, struct_name));
            }
            
            code.push_str(&format!("        {}() = delete; /* disable default constructor, use C++20 designated initializer instead */\r\n", struct_name));
            
            code.push_str("    };\r\n\r\n");
        } else if let Some(enum_fields) = &class_data.enum_fields {
            if !enum_is_union(enum_fields) {
                code.push_str(&format!("    enum class {} {{\r\n", struct_name));
                
                for variant_map in enum_fields {
                    for (variant_name, _) in variant_map {
                        code.push_str(&format!("        {},\r\n", variant_name));
                    }
                }
                
                code.push_str("    };\r\n\r\n");
            } else {
                // Generate tag enum for tagged union
                code.push_str(&format!("    enum class {}Tag {{\r\n", struct_name));
                
                for variant_map in enum_fields {
                    for (variant_name, _) in variant_map {
                        code.push_str(&format!("        {},\r\n", variant_name));
                    }
                }
                
                code.push_str("    };\r\n\r\n");
                
                // Generate variant structs for tagged union
                for variant_map in enum_fields {
                    for (variant_name, variant_data) in variant_map {
                        code.push_str(&format!("    struct {}Variant_{} {{ {}Tag tag;", struct_name, variant_name, struct_name));
                        
                        if let Some(variant_type) = &variant_data.r#type {
                            let (prefix, base_type, suffix) = analyze_type(variant_type);
                            
                            if is_primitive_arg(&base_type) {
                                let c_type = replace_primitive_ctype(&base_type);
                                code.push_str(&format!(" {}{}{} payload;", 
                                                     c_type,
                                                     replace_primitive_ctype(&prefix),
                                                     suffix));
                            } else if let Some((_, type_class_name)) = search_for_class_by_class_name(api_data, &base_type) {
                                code.push_str(&format!(" {}{}{} payload;", 
                                                     type_class_name,
                                                     replace_primitive_ctype(&prefix),
                                                     suffix));
                            }
                        }
                        
                        code.push_str(" };\r\n\r\n");
                    }
                }
                
                // Generate the union itself
                code.push_str(&format!("    union {} {{\r\n", struct_name));
                
                for variant_map in enum_fields {
                    for (variant_name, _) in variant_map {
                        code.push_str(&format!("        {}Variant_{} {};\r\n", struct_name, variant_name, variant_name));
                    }
                }
                
                code.push_str("    };\r\n\r\n");
            }
        }
    }
    
    // Generate function declarations
    code.push_str("    /* FUNCTIONS */\r\n\r\n");
    code.push_str("    extern \"C\" {\r\n");
    
    for (module_name, module) in &version_data.modules {
        for (class_name, class_data) in &module.classes {
            let class_ptr_name = format!("{}", class_name); // No prefix in C++
            let c_is_stack_allocated = class_is_stack_allocated(class_data);
            let class_can_be_copied = class_data.derive.as_ref().map_or(false, |d| d.contains(&"Copy".to_string()));
            let class_has_recursive_destructor = has_recursive_destructor(api_data, class_data);
            let class_has_custom_destructor = class_data.custom_destructor.unwrap_or(false);
            let treat_external_as_ptr = class_data.external.is_some() && class_data.is_boxed_object;
            let class_can_be_cloned = class_data.clone.unwrap_or(true);
            
            // Generate constructors
            if let Some(constructors) = &class_data.constructors {
                for (fn_name, constructor) in constructors {
                    let c_fn_name = format!("{}_{}", class_name, snake_case_to_lower_camel(fn_name));
                    
                    // Generate simplified function arguments
                    let fn_args = "/* function args */";
                    
                    // Generate simplified return type
                    let returns = class_ptr_name.clone();
                    
                    code.push_str(&format!("        {} {}({});\r\n", returns, c_fn_name, fn_args));
                }
            }
            
            // Generate methods
            if let Some(functions) = &class_data.functions {
                for (fn_name, function) in functions {
                    let c_fn_name = format!("{}_{}", class_name, snake_case_to_lower_camel(fn_name));
                    
                    // Generate simplified function arguments
                    let fn_args = "/* function args */";
                    
                    // Generate simplified return type
                    let returns = if function.returns.is_some() {
                        "/* return type */"
                    } else {
                        "void"
                    };
                    
                    code.push_str(&format!("        {} {}({});\r\n", returns, c_fn_name, fn_args));
                }
            }
            
            // Generate destructor and deep copy methods
            if c_is_stack_allocated {
                if !class_can_be_copied && (class_has_custom_destructor || treat_external_as_ptr || class_has_recursive_destructor) {
                    code.push_str(&format!("        void {}_delete({}* instance);\r\n", class_name, class_name));
                }
                
                if treat_external_as_ptr && class_can_be_cloned {
                    code.push_str(&format!("        {} {}_deepCopy(const {}* instance);\r\n", class_name, class_name, class_name));
                }
            }
            
            code.push_str("\r\n");
        }
    }
    
    code.push_str("    } /* extern \"C\" */\r\n");
    
    // Close the namespace and header file
    code.push_str("\r\n");
    code.push_str("} /* namespace dll */\r\n");
    code.push_str("\r\n#endif /* AZUL_H */\r\n");
    
    code
}

/// Collect and sort struct definitions
fn collect_structs(api_data: &ApiData) -> IndexMap<String, &crate::api::ClassData> {
    let mut structs = IndexMap::new();
    
    // Get the latest version
    let latest_version = api_data.get_latest_version_str().unwrap();
    let version_data = api_data.get_version(latest_version).unwrap();
    
    // Collect all classes from all modules
    for (module_name, module) in &version_data.modules {
        for (class_name, class_data) in &module.classes {
            // In C++, we don't use the prefix in the type name
            structs.insert(class_name.clone(), class_data);
        }
    }
    
    // This is a simplification - in the real implementation, we'd need to sort
    // the structs based on dependencies to avoid forward declarations
    structs
}