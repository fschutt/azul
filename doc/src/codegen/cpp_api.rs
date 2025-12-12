use indexmap::IndexMap;

use crate::{
    api::{ApiData, ClassData, VersionData},
    utils::{
        analyze::{
            analyze_type, class_is_stack_allocated, enum_is_union, has_recursive_destructor,
            is_primitive_arg, replace_primitive_ctype, search_for_class_by_class_name,
        },
        string::snake_case_to_lower_camel,
    },
    codegen::c_api::{escape_cpp_keyword, sort_structs_by_dependencies},
};

const C_PREFIX: &str = "Az";

/// C++ language version
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CppVersion {
    Cpp03,
    Cpp11,
    Cpp14,
    Cpp17,
    Cpp20,
    Cpp23,
}

impl CppVersion {
    /// Get all supported C++ versions
    pub fn all() -> &'static [CppVersion] {
        &[
            CppVersion::Cpp03,
            CppVersion::Cpp11,
            CppVersion::Cpp14,
            CppVersion::Cpp17,
            CppVersion::Cpp20,
            CppVersion::Cpp23,
        ]
    }
    
    /// Get the version number as a string (e.g., "03", "11")
    pub fn version_number(&self) -> &'static str {
        match self {
            CppVersion::Cpp03 => "03",
            CppVersion::Cpp11 => "11",
            CppVersion::Cpp14 => "14",
            CppVersion::Cpp17 => "17",
            CppVersion::Cpp20 => "20",
            CppVersion::Cpp23 => "23",
        }
    }
    
    /// Get the standard flag for the compiler (e.g., "-std=c++11")
    pub fn standard_flag(&self) -> &'static str {
        match self {
            CppVersion::Cpp03 => "-std=c++03",
            CppVersion::Cpp11 => "-std=c++11",
            CppVersion::Cpp14 => "-std=c++14",
            CppVersion::Cpp17 => "-std=c++17",
            CppVersion::Cpp20 => "-std=c++20",
            CppVersion::Cpp23 => "-std=c++23",
        }
    }
    
    /// Get the header filename for this version (just the filename, no path)
    pub fn header_filename(&self) -> String {
        format!("azul_cpp{}.hpp", self.version_number())
    }
    
    /// Check if this version supports move semantics (C++11+)
    pub fn has_move_semantics(&self) -> bool {
        !matches!(self, CppVersion::Cpp03)
    }
    
    /// Check if this version supports noexcept (C++11+)
    pub fn has_noexcept(&self) -> bool {
        !matches!(self, CppVersion::Cpp03)
    }
    
    /// Check if this version supports std::optional (C++17+)
    pub fn has_optional(&self) -> bool {
        matches!(self, CppVersion::Cpp17 | CppVersion::Cpp20 | CppVersion::Cpp23)
    }
    
    /// Check if this version supports std::variant (C++17+)
    pub fn has_variant(&self) -> bool {
        matches!(self, CppVersion::Cpp17 | CppVersion::Cpp20 | CppVersion::Cpp23)
    }
    
    /// Check if this version supports std::span (C++20+)
    pub fn has_span(&self) -> bool {
        matches!(self, CppVersion::Cpp20 | CppVersion::Cpp23)
    }
    
    /// Check if this version supports [[nodiscard]] (C++17+)
    pub fn has_nodiscard(&self) -> bool {
        matches!(self, CppVersion::Cpp17 | CppVersion::Cpp20 | CppVersion::Cpp23)
    }
    
    /// Check if this version supports auto return type deduction (C++14+)
    pub fn has_auto_return(&self) -> bool {
        !matches!(self, CppVersion::Cpp03 | CppVersion::Cpp11)
    }
    
    /// Check if this version supports std::string_view (C++17+)
    pub fn has_string_view(&self) -> bool {
        matches!(self, CppVersion::Cpp17 | CppVersion::Cpp20 | CppVersion::Cpp23)
    }
}

/// Generate C++ API code from API data (default version: C++11)
pub fn generate_cpp_api(api_data: &ApiData, version: &str) -> String {
    generate_cpp_api_versioned(api_data, version, CppVersion::Cpp11)
}

/// Check if a class is Copy (can be trivially copied)
fn class_is_copy(class_data: &ClassData) -> bool {
    class_data
        .derive
        .as_ref()
        .map_or(false, |d| d.contains(&"Copy".to_string()))
}

/// Check if a class is a Vec type (has ptr, len, cap, destructor fields)
fn class_is_vec_type(class_data: &ClassData) -> bool {
    if let Some(struct_fields) = &class_data.struct_fields {
        let field_names: Vec<&str> = struct_fields.iter()
            .flat_map(|field_map| field_map.keys().map(|s| s.as_str()))
            .collect();
        field_names.contains(&"ptr") && field_names.contains(&"len") && 
            field_names.contains(&"cap") && field_names.contains(&"destructor")
    } else {
        false
    }
}

/// Get the element type of a Vec type (from the ptr field type)
fn get_vec_element_type(class_data: &ClassData) -> Option<String> {
    if let Some(struct_fields) = &class_data.struct_fields {
        for field_map in struct_fields {
            if let Some(ptr_field) = field_map.get("ptr") {
                // The ptr field should have the element type (e.g., "Dom" not "c_void")
                let elem_type = &ptr_field.r#type;
                // Skip if it's c_void - this means autofix didn't expand impl_vec! properly
                if elem_type == "c_void" {
                    return None;
                }
                return Some(elem_type.clone());
            }
        }
    }
    None
}

/// Check if a class is a String type
fn class_is_string_type(class_name: &str) -> bool {
    class_name == "String"
}

/// Check if a class is an Option type (starts with "Option" and has Some/None variants)
fn class_is_option_type(class_name: &str, class_data: &ClassData) -> bool {
    if !class_name.starts_with("Option") {
        return false;
    }
    if let Some(enum_fields) = &class_data.enum_fields {
        let variant_names: Vec<&str> = enum_fields.iter()
            .flat_map(|field_map| field_map.keys().map(|s| s.as_str()))
            .collect();
        variant_names.contains(&"Some") && variant_names.contains(&"None")
    } else {
        false
    }
}

/// Get the inner type of an Option type (from the Some variant)
fn get_option_inner_type(class_data: &ClassData) -> Option<String> {
    if let Some(enum_fields) = &class_data.enum_fields {
        for field_map in enum_fields {
            if let Some(some_variant) = field_map.get("Some") {
                if let Some(ty) = &some_variant.r#type {
                    return Some(ty.clone());
                }
            }
        }
    }
    None
}

/// Check if a class is a Result type (starts with "Result" and has Ok/Err variants)
fn class_is_result_type(class_name: &str, class_data: &ClassData) -> bool {
    if !class_name.starts_with("Result") {
        return false;
    }
    if let Some(enum_fields) = &class_data.enum_fields {
        let variant_names: Vec<&str> = enum_fields.iter()
            .flat_map(|field_map| field_map.keys().map(|s| s.as_str()))
            .collect();
        variant_names.contains(&"Ok") && variant_names.contains(&"Err")
    } else {
        false
    }
}

/// Get the Ok and Err types of a Result type
fn get_result_types(class_data: &ClassData) -> Option<(String, String)> {
    if let Some(enum_fields) = &class_data.enum_fields {
        let mut ok_type = None;
        let mut err_type = None;
        for field_map in enum_fields {
            if let Some(ok_variant) = field_map.get("Ok") {
                ok_type = ok_variant.r#type.clone();
            }
            if let Some(err_variant) = field_map.get("Err") {
                err_type = err_variant.r#type.clone();
            }
        }
        if let (Some(ok), Some(err)) = (ok_type, err_type) {
            return Some((ok, err));
        }
    }
    None
}

/// Check if a class needs a destructor
fn class_needs_destructor(class_data: &ClassData, version_data: &VersionData) -> bool {
    if class_is_copy(class_data) {
        return false;
    }
    
    let class_has_custom_destructor = class_data.custom_destructor.unwrap_or(false);
    let treat_external_as_ptr = class_data.external.is_some() && class_data.is_boxed_object;
    let class_has_recursive_destructor = has_recursive_destructor(version_data, class_data);
    let class_has_custom_drop = class_data
        .custom_impls
        .as_ref()
        .map_or(false, |impls| impls.contains(&"Drop".to_string()));
    
    class_has_custom_destructor
        || treat_external_as_ptr
        || class_has_recursive_destructor
        || class_has_custom_drop
}

/// Check if a class has Clone
fn class_has_clone(class_data: &ClassData) -> bool {
    class_data
        .custom_impls
        .as_ref()
        .map_or(false, |impls| impls.contains(&"Clone".to_string()))
        || class_data
            .derive
            .as_ref()
            .map_or(false, |d| d.contains(&"Clone".to_string()))
}

/// Check if an enum is a "simple" enum (no variants have data)
fn class_is_simple_enum(class_data: &ClassData) -> bool {
    if let Some(enum_fields) = &class_data.enum_fields {
        for variant_map in enum_fields {
            for (_variant_name, variant_data) in variant_map {
                if variant_data.r#type.is_some() {
                    return false;
                }
            }
        }
        return true;
    }
    false
}

/// Check if a type has a real C++ wrapper class (not a type alias or callback)
fn type_has_cpp_wrapper(type_name: &str, version_data: &VersionData) -> bool {
    use crate::utils::analyze::get_class;
    
    if let Some((module_name, class_name)) = search_for_class_by_class_name(version_data, type_name) {
        if let Some(class_data) = get_class(version_data, module_name, class_name) {
            // Callback typedefs don't get a wrapper
            if class_data.callback_typedef.is_some() {
                return false;
            }
            // Type aliases without struct fields don't get a wrapper
            if class_data.type_alias.is_some() && class_data.struct_fields.is_none() {
                return false;
            }
            // Simple enums only get a type alias, not a wrapper class
            if class_is_simple_enum(class_data) {
                return false;
            }
            return true;
        }
    }
    false
}

/// Check if a type needs the Proxy path for C++03 (non-copy wrapper class)
fn type_needs_proxy_for_cpp03(type_name: &str, version_data: &VersionData) -> bool {
    use crate::utils::analyze::get_class;
    
    if !type_has_cpp_wrapper(type_name, version_data) {
        return false;
    }
    
    if let Some((module_name, class_name)) = search_for_class_by_class_name(version_data, type_name) {
        if let Some(class_data) = get_class(version_data, module_name, class_name) {
            // Copy types don't need Proxy
            return !class_is_copy(class_data);
        }
    }
    false
}

/// Alias for generate_cpp_api_versioned (for compatibility)
pub fn generate_cpp_api_for_version(api_data: &ApiData, version: &str, cpp_version: CppVersion) -> String {
    generate_cpp_api_versioned(api_data, version, cpp_version)
}

/// Generate C++ API code for a specific C++ version
pub fn generate_cpp_api_versioned(api_data: &ApiData, version: &str, cpp_version: CppVersion) -> String {
    let mut code = String::new();
    let version_data = api_data.get_version(version).unwrap();
    
    // Header comment
    code.push_str("// =============================================================================\r\n");
    code.push_str(&format!("// Azul C++{} API Wrapper\r\n", cpp_version.version_number()));
    code.push_str("// =============================================================================\r\n");
    code.push_str("//\r\n");
    code.push_str(&format!("// Compile with: g++ {} -o myapp myapp.cpp -lazul\r\n", cpp_version.standard_flag()));
    code.push_str("//\r\n");
    code.push_str("// This header provides C++ wrapper classes for the Azul C API.\r\n");
    code.push_str("// All classes use RAII for memory management.\r\n");
    code.push_str("//\r\n");
    
    // Add C++03 specific documentation
    if !cpp_version.has_move_semantics() {
        code.push_str("// C++03 MOVE EMULATION (Colvin-Gibbons Trick)\r\n");
        code.push_str("// ============================================\r\n");
        code.push_str("//\r\n");
        code.push_str("// C++03 lacks move semantics, which normally prevents returning non-copyable\r\n");
        code.push_str("// RAII objects by value. This header uses the \"Colvin-Gibbons trick\" (also\r\n");
        code.push_str("// known as the \"Move Constructor Idiom\") to work around this limitation.\r\n");
        code.push_str("//\r\n");
        code.push_str("// This is essentially a manual, type-safe implementation of std::auto_ptr's\r\n");
        code.push_str("// ownership transfer semantics, but without auto_ptr's pitfalls.\r\n");
        code.push_str("//\r\n");
        code.push_str("// How it works:\r\n");
        code.push_str("// - Each non-copyable class has a nested 'Proxy' struct\r\n");
        code.push_str("// - When returning by value, the object converts to Proxy (releasing ownership)\r\n");
        code.push_str("// - The receiving object constructs from Proxy (acquiring ownership)\r\n");
        code.push_str("// - Direct copy construction transfers ownership (like std::auto_ptr)\r\n");
        code.push_str("//\r\n");
        code.push_str("// Example:\r\n");
        code.push_str("//   String a = String::fromConstStr(\"hello\");  // OK: uses Proxy path\r\n");
        code.push_str("//   String b = a;                               // OK but TRANSFERS ownership from a!\r\n");
        code.push_str("//   // 'a' is now in a moved-from (zombie) state\r\n");
        code.push_str("//   String c = String::fromConstStr(\"world\");  // OK\r\n");
        code.push_str("//\r\n");
        code.push_str("// WARNING: STL CONTAINER INCOMPATIBILITY\r\n");
        code.push_str("// =====================================\r\n");
        code.push_str("// These objects CANNOT be safely stored in C++03 standard containers\r\n");
        code.push_str("// (std::vector, std::list, std::map, etc.)!\r\n");
        code.push_str("//\r\n");
        code.push_str("// C++03 containers require \"Copy Constructible\" and \"Assignable\" semantics\r\n");
        code.push_str("// where copying A to B leaves both A and B as equivalent copies.\r\n");
        code.push_str("// These wrappers use destructive copy (like std::auto_ptr), which violates\r\n");
        code.push_str("// this requirement and WILL cause memory corruption during container\r\n");
        code.push_str("// operations like resize, sort, or internal reallocation.\r\n");
        code.push_str("//\r\n");
        code.push_str("// Safe alternatives:\r\n");
        code.push_str("// 1. Store raw pointers and manage lifetime manually\r\n");
        code.push_str("// 2. Use C arrays with manual size tracking\r\n");
        code.push_str("// 3. Upgrade to C++11 where move semantics work correctly with containers\r\n");
        code.push_str("//\r\n");
        code.push_str("// Reference: https://en.wikibooks.org/wiki/More_C%2B%2B_Idioms/Move_Constructor\r\n");
        code.push_str("//\r\n");
    }
    
    code.push_str("// =============================================================================\r\n");
    code.push_str("\r\n");
    
    // Include guards
    code.push_str(&format!("#ifndef AZUL_CPP{}_HPP\r\n", cpp_version.version_number()));
    code.push_str(&format!("#define AZUL_CPP{}_HPP\r\n", cpp_version.version_number()));
    code.push_str("\r\n");
    
    // Include the C header
    code.push_str("extern \"C\" {\r\n");
    code.push_str("#include <azul.h>\r\n");
    code.push_str("}\r\n");
    code.push_str("\r\n");
    
    // Standard includes
    code.push_str("#include <cstdint>\r\n");
    code.push_str("#include <cstddef>\r\n");
    code.push_str("#include <cstring>\r\n");
    if cpp_version.has_move_semantics() {
        code.push_str("#include <utility>\r\n");
        code.push_str("#include <stdexcept>\r\n");
        code.push_str("#include <string>\r\n");  // For std::string interop
        code.push_str("#include <vector>\r\n");  // For toStdVector()
    }
    if cpp_version.has_optional() {
        code.push_str("#include <optional>\r\n");
    }
    if cpp_version.has_variant() {
        code.push_str("#include <variant>\r\n");
    }
    if cpp_version.has_span() {
        code.push_str("#include <span>\r\n");
    }
    code.push_str("\r\n");
    
    // Open namespace
    code.push_str("namespace azul {\r\n");
    code.push_str("\r\n");
    
    // Sort structs by dependencies
    let sorted = sort_structs_by_dependencies(api_data, version, "").unwrap();
    
    // Forward declarations for all classes
    code.push_str("// Forward declarations\r\n");
    for (struct_name, class_data) in &sorted.structs {
        // Skip generic types - they are templates for monomorphized versions
        if class_data.generic_params.is_some() {
            continue;
        }
        if class_data.callback_typedef.is_some() {
            continue;
        }
        if class_data.type_alias.is_some() && class_data.struct_fields.is_none() {
            continue;
        }
        if class_is_simple_enum(class_data) {
            continue;
        }
        code.push_str(&format!("class {};\r\n", struct_name));
    }
    code.push_str("\r\n");
    
    // Generate wrapper classes (declarations only, no method bodies)
    code.push_str("// Wrapper class declarations\r\n");
    code.push_str("\r\n");
    
    for (struct_name, class_data) in &sorted.structs {
        generate_cpp_class_declaration(
            &mut code,
            struct_name,
            class_data,
            version_data,
            cpp_version,
        );
    }
    
    // Generate method implementations after all classes are declared
    code.push_str("// Method implementations\r\n");
    code.push_str("// (Implemented after all classes are declared to avoid incomplete type errors)\r\n");
    code.push_str("\r\n");
    
    for (struct_name, class_data) in &sorted.structs {
        generate_cpp_method_implementations(
            &mut code,
            struct_name,
            class_data,
            version_data,
            cpp_version,
        );
    }
    
    // Close namespace
    code.push_str("} // namespace azul\r\n");
    code.push_str("\r\n");
    
    // End include guards
    code.push_str(&format!("#endif // AZUL_CPP{}_HPP\r\n", cpp_version.version_number()));
    
    code
}

/// Generate C++ class declaration (without method bodies)
fn generate_cpp_class_declaration(
    code: &mut String,
    class_name: &str,
    class_data: &ClassData,
    version_data: &VersionData,
    cpp_version: CppVersion,
) {
    let c_type_name = format!("{}{}", C_PREFIX, class_name);
    let needs_destructor = class_needs_destructor(class_data, version_data);
    let is_copy = class_is_copy(class_data);
    
    // Skip generic types - they are templates for monomorphized versions
    if class_data.generic_params.is_some() {
        return;
    }
    
    // Skip callback typedefs
    if class_data.callback_typedef.is_some() {
        return;
    }
    
    // Skip type aliases without struct fields
    if class_data.type_alias.is_some() && class_data.struct_fields.is_none() {
        return;
    }
    
    if class_is_simple_enum(class_data) {
        if cpp_version.has_move_semantics() {
            code.push_str(&format!("using {} = {};\r\n\r\n", class_name, c_type_name));
        } else {
            code.push_str(&format!("typedef {} {};\r\n\r\n", c_type_name, class_name));
        }
        return;
    }
    
    // Class declaration
    code.push_str(&format!("class {} {{\r\n", class_name));
    
    if !cpp_version.has_move_semantics() && !is_copy {
        code.push_str("public:\r\n");
        code.push_str(&format!("    struct Proxy {{ {} inner; Proxy({} p) : inner(p) {{}} }};\r\n", c_type_name, c_type_name));
        code.push_str("\r\n");
    }
    
    code.push_str("private:\r\n");
    // For C++03 non-Copy types: use mutable to allow "stealing" in const copy constructor (like std::auto_ptr)
    // For C++11+ and Copy types: no mutable needed
    if !cpp_version.has_move_semantics() && !is_copy {
        code.push_str(&format!("    mutable {} inner_;\r\n", c_type_name));
    } else {
        code.push_str(&format!("    {} inner_;\r\n", c_type_name));
    }
    code.push_str("\r\n");
    
    // Disable copy if not Copy type
    if !is_copy {
        if cpp_version.has_move_semantics() {
            code.push_str(&format!("    {}(const {}&) = delete;\r\n", class_name, class_name));
            code.push_str(&format!("    {}& operator=(const {}&) = delete;\r\n", class_name, class_name));
        }
        // For C++03: Copy constructor is defined in public section with destructive copy semantics
    }
    
    code.push_str("\r\n");
    code.push_str("public:\r\n");
    
    // Constructor from C type (inline - no external dependencies)
    let noexcept = if cpp_version.has_noexcept() { " noexcept" } else { "" };
    code.push_str(&format!("    explicit {}({} inner){} : inner_(inner) {{}}\r\n", 
        class_name, c_type_name, noexcept));
    
    // For Copy types: generate copy constructor and assignment operator
    if is_copy {
        code.push_str(&format!("    {}(const {}& other){} : inner_(other.inner_) {{}}\r\n",
            class_name, class_name, noexcept));
        code.push_str(&format!("    {}& operator=(const {}& other){} {{ inner_ = other.inner_; return *this; }}\r\n",
            class_name, class_name, noexcept));
    }
    
    // C++03: Destructive copy constructor (like std::auto_ptr)
    // Uses mutable inner_ so we can "steal" in const& constructor
    // This is necessary because C++03 requires copy constructor to be accessible for RVO
    if !cpp_version.has_move_semantics() && !is_copy {
        // const& version needed for returning temporaries (RVO requires accessible copy ctor)
        code.push_str(&format!("    {}(const {}& other) : inner_(other.inner_) {{ std::memset(&other.inner_, 0, sizeof(other.inner_)); }}\r\n", 
            class_name, class_name));
        code.push_str(&format!("    {}& operator=(const {}& other) {{\r\n", class_name, class_name));
        if needs_destructor {
            code.push_str(&format!("        {}_delete(&inner_);\r\n", c_type_name));
        }
        code.push_str("        inner_ = other.inner_;\r\n");
        code.push_str("        std::memset(&other.inner_, 0, sizeof(other.inner_));\r\n");
        code.push_str("        return *this;\r\n");
        code.push_str("    }\r\n");
        code.push_str(&format!("    {}(Proxy p) : inner_(p.inner) {{}}\r\n", class_name));
        code.push_str("    operator Proxy() {\r\n");
        code.push_str("        Proxy p(inner_);\r\n");
        code.push_str("        std::memset(&inner_, 0, sizeof(inner_));\r\n");
        code.push_str("        return p;\r\n");
        code.push_str("    }\r\n");
        code.push_str(&format!("    {}& operator=(Proxy p) {{\r\n", class_name));
        if needs_destructor {
            code.push_str(&format!("        {}_delete(&inner_);\r\n", c_type_name));
        }
        code.push_str("        inner_ = p.inner;\r\n");
        code.push_str("        return *this;\r\n");
        code.push_str("    }\r\n");
    }
    
    // Destructor (inline - no external dependencies)
    if needs_destructor {
        code.push_str(&format!("    ~{}() {{ {}_delete(&inner_); }}\r\n", 
            class_name, c_type_name));
    } else {
        code.push_str(&format!("    ~{}() {{}}\r\n", class_name));
    }
    
    // Move semantics (inline - only uses own type) - C++11+ only
    if cpp_version.has_move_semantics() && !is_copy {
        code.push_str("\r\n");
        code.push_str(&format!("    {}({}&& other) noexcept : inner_(other.inner_) {{\r\n", 
            class_name, class_name));
        code.push_str("        other.inner_ = {};\r\n");
        code.push_str("    }\r\n");
        
        code.push_str(&format!("    {}& operator=({}&& other) noexcept {{\r\n", 
            class_name, class_name));
        code.push_str("        if (this != &other) {\r\n");
        if needs_destructor {
            code.push_str(&format!("            {}_delete(&inner_);\r\n", c_type_name));
        }
        code.push_str("            inner_ = other.inner_;\r\n");
        code.push_str("            other.inner_ = {};\r\n");
        code.push_str("        }\r\n");
        code.push_str("        return *this;\r\n");
        code.push_str("    }\r\n");
    }
    
    if let Some(constructors) = &class_data.constructors {
        code.push_str("\r\n");
        for (fn_name, constructor) in constructors {
            let cpp_fn_name = if fn_name == "new" {
                "new_".to_string()
            } else if fn_name == "default" {
                "default_".to_string()
            } else {
                snake_case_to_lower_camel(fn_name)
            };
            
            let cpp_args = generate_cpp_args_signature(constructor, version_data, cpp_version, false);
            let nodiscard = if cpp_version.has_nodiscard() { "[[nodiscard]] " } else { "" };
            
            code.push_str(&format!("    {}static {} {}({});\r\n", 
                nodiscard, class_name, cpp_fn_name, cpp_args));
        }
    }
    
    if let Some(functions) = &class_data.functions {
        code.push_str("\r\n");
        for (fn_name, function) in functions {
            // Escape C++ reserved keywords in method names (e.g., "union" -> "union_")
            let cpp_fn_name = escape_cpp_keyword(&snake_case_to_lower_camel(fn_name));
            
            let is_const = function.fn_args.first()
                .and_then(|arg| arg.iter().next())
                .map(|(name, typ)| name == "self" && (typ == "ref" || typ == "value"))
                .unwrap_or(false);
            
            let cpp_return_type = if let Some(ret) = &function.returns {
                get_cpp_return_type(&ret.r#type, version_data)
            } else {
                "void".to_string()
            };
            
            let cpp_args = generate_cpp_args_signature(function, version_data, cpp_version, true);
            let const_suffix = if is_const { " const" } else { "" };
            
            code.push_str(&format!("    {} {}({}){};\r\n", 
                cpp_return_type, cpp_fn_name, cpp_args, const_suffix));
        }
    }
    
    code.push_str("\r\n");
    // Const-correct accessor methods
    code.push_str(&format!("    const {}& inner() const {{ return inner_; }}\r\n", c_type_name));
    code.push_str(&format!("    {}& inner() {{ return inner_; }}\r\n", c_type_name));
    code.push_str(&format!("    const {}* ptr() const {{ return &inner_; }}\r\n", c_type_name));
    code.push_str(&format!("    {}* ptr() {{ return &inner_; }}\r\n", c_type_name));
    
    // release() implementation depends on C++ version
    if cpp_version.has_move_semantics() {
        // C++11+: use brace initialization
        code.push_str(&format!("    {} release() {{ {} result = inner_; inner_ = {{}}; return result; }}\r\n", 
            c_type_name, c_type_name));
    } else {
        // C++03: use memset to zero initialize
        code.push_str(&format!("    {} release() {{ {} result = inner_; std::memset(&inner_, 0, sizeof(inner_)); return result; }}\r\n", 
            c_type_name, c_type_name));
    }
    
    // Vec types: add iterator support for range-based for loops
    let is_vec = class_is_vec_type(class_data);
    if is_vec {
        if let Some(elem_type) = get_vec_element_type(class_data) {
            let c_elem_type = if is_primitive_arg(&elem_type) {
                replace_primitive_ctype(&elem_type)
            } else {
                format!("{}{}", C_PREFIX, elem_type)
            };
            
            code.push_str("\r\n");
            code.push_str("    // Iterator support for range-based for loops\r\n");
            // ptr is already `const T*`, so we can use it directly
            code.push_str(&format!("    const {}* begin() const {{ return inner_.ptr; }}\r\n", c_elem_type));
            code.push_str(&format!("    const {}* end() const {{ return inner_.ptr + inner_.len; }}\r\n", c_elem_type));
            // For mutable access, cast away const (the C API uses const ptr for both)
            code.push_str(&format!("    {}* begin() {{ return const_cast<{}*>(inner_.ptr); }}\r\n", c_elem_type, c_elem_type));
            code.push_str(&format!("    {}* end() {{ return const_cast<{}*>(inner_.ptr) + inner_.len; }}\r\n", c_elem_type, c_elem_type));
            code.push_str("    size_t size() const { return inner_.len; }\r\n");
            code.push_str("    bool empty() const { return inner_.len == 0; }\r\n");
            code.push_str(&format!("    const {}& operator[](size_t i) const {{ return inner_.ptr[i]; }}\r\n", c_elem_type));
            code.push_str(&format!("    {}& operator[](size_t i) {{ return const_cast<{}*>(inner_.ptr)[i]; }}\r\n", c_elem_type, c_elem_type));
            
            // C++11+: add toStdVector() helper for conversion to std::vector
            if cpp_version.has_move_semantics() {
                code.push_str(&format!("    std::vector<{}> toStdVector() const {{ return std::vector<{}>(begin(), end()); }}\r\n", 
                    c_elem_type, c_elem_type));
            }
        }
    }
    
    // String type: add std::string interop
    if class_is_string_type(class_name) {
        code.push_str("\r\n");
        code.push_str("    // std::string interoperability\r\n");
        if cpp_version.has_move_semantics() {
            code.push_str("    String(const char* s) : inner_(AzString_copyFromBytes(reinterpret_cast<const uint8_t*>(s), 0, std::strlen(s))) {}\r\n");
            code.push_str("    String(const std::string& s) : inner_(AzString_copyFromBytes(reinterpret_cast<const uint8_t*>(s.c_str()), 0, s.size())) {}\r\n");
        } else {
            code.push_str("    explicit String(const char* s) : inner_(AzString_copyFromBytes(reinterpret_cast<const uint8_t*>(s), 0, std::strlen(s))) {}\r\n");
        }
        code.push_str("    const char* c_str() const { return reinterpret_cast<const char*>(inner_.vec.ptr); }\r\n");
        code.push_str("    size_t length() const { return inner_.vec.len; }\r\n");
        if cpp_version.has_move_semantics() {
            code.push_str("    std::string toStdString() const { return std::string(c_str(), length()); }\r\n");
            code.push_str("    operator std::string() const { return toStdString(); }\r\n");
        }
    }
    
    // Option types: add convenience methods
    if class_is_option_type(class_name, class_data) {
        if let Some(inner_type) = get_option_inner_type(class_data) {
            let c_inner_type = if is_primitive_arg(&inner_type) {
                replace_primitive_ctype(&inner_type)
            } else {
                format!("{}{}", C_PREFIX, inner_type)
            };
            
            code.push_str("\r\n");
            code.push_str("    // Option convenience methods\r\n");
            // The tag is stored in the None/Some variant structs, but at the same offset
            // So we can access it via either variant (using Some.tag here)
            code.push_str(&format!("    bool isSome() const {{ return inner_.Some.tag == {}_Tag_Some; }}\r\n", c_type_name));
            code.push_str(&format!("    bool isNone() const {{ return inner_.Some.tag == {}_Tag_None; }}\r\n", c_type_name));
            
            // For C++11+, provide explicit bool conversion
            if cpp_version.has_move_semantics() {
                code.push_str("    explicit operator bool() const { return isSome(); }\r\n");
            }
            
            // unwrap() - returns the inner value, undefined behavior if None
            code.push_str(&format!("    const {}& unwrap() const {{ return inner_.Some.payload; }}\r\n", c_inner_type));
            code.push_str(&format!("    {}& unwrap() {{ return inner_.Some.payload; }}\r\n", c_inner_type));
            
            // unwrapOr() - returns the inner value or a default
            code.push_str(&format!("    {} unwrapOr(const {}& defaultValue) const {{ return isSome() ? inner_.Some.payload : defaultValue; }}\r\n", 
                c_inner_type, c_inner_type));
        }
    }
    
    // Result types: add convenience methods
    if class_is_result_type(class_name, class_data) {
        if let Some((ok_type, err_type)) = get_result_types(class_data) {
            let c_ok_type = if is_primitive_arg(&ok_type) {
                replace_primitive_ctype(&ok_type)
            } else {
                format!("{}{}", C_PREFIX, ok_type)
            };
            let c_err_type = if is_primitive_arg(&err_type) {
                replace_primitive_ctype(&err_type)
            } else {
                format!("{}{}", C_PREFIX, err_type)
            };
            
            code.push_str("\r\n");
            code.push_str("    // Result convenience methods\r\n");
            // The tag is stored in the Ok/Err variant structs, but at the same offset
            code.push_str(&format!("    bool isOk() const {{ return inner_.Ok.tag == {}_Tag_Ok; }}\r\n", c_type_name));
            code.push_str(&format!("    bool isErr() const {{ return inner_.Ok.tag == {}_Tag_Err; }}\r\n", c_type_name));
            
            // For C++11+, provide explicit bool conversion (true = Ok)
            if cpp_version.has_move_semantics() {
                code.push_str("    explicit operator bool() const { return isOk(); }\r\n");
            }
            
            // unwrap() - returns the Ok value, undefined behavior if Err
            code.push_str(&format!("    const {}& unwrap() const {{ return inner_.Ok.payload; }}\r\n", c_ok_type));
            code.push_str(&format!("    {}& unwrap() {{ return inner_.Ok.payload; }}\r\n", c_ok_type));
            
            // unwrapErr() - returns the Err value, undefined behavior if Ok
            code.push_str(&format!("    const {}& unwrapErr() const {{ return inner_.Err.payload; }}\r\n", c_err_type));
            code.push_str(&format!("    {}& unwrapErr() {{ return inner_.Err.payload; }}\r\n", c_err_type));
        }
    }
    
    code.push_str("};\r\n\r\n");
}

/// Generate method implementations outside the class
fn generate_cpp_method_implementations(
    code: &mut String,
    class_name: &str,
    class_data: &ClassData,
    version_data: &VersionData,
    cpp_version: CppVersion,
) {
    let c_type_name = format!("{}{}", C_PREFIX, class_name);
    
    // Skip generic types - they are templates for monomorphized versions
    if class_data.generic_params.is_some() {
        return;
    }
    
    // Skip types that don't get class wrappers
    if class_data.callback_typedef.is_some() {
        return;
    }
    if class_data.type_alias.is_some() && class_data.struct_fields.is_none() {
        return;
    }
    if class_is_simple_enum(class_data) {
        return;
    }
    
    // Static constructor implementations
    if let Some(constructors) = &class_data.constructors {
        // For C++03 non-copy types, use Proxy path for return values
        let use_proxy = !cpp_version.has_move_semantics() && !class_is_copy(class_data);
        
        for (fn_name, constructor) in constructors {
            let cpp_fn_name = if fn_name == "new" {
                "new_".to_string()
            } else if fn_name == "default" {
                "default_".to_string()
            } else {
                snake_case_to_lower_camel(fn_name)
            };
            
            let c_fn_name = format!("{}_{}", c_type_name, snake_case_to_lower_camel(fn_name));
            let cpp_args = generate_cpp_args_signature(constructor, version_data, cpp_version, false);
            let call_args = generate_cpp_call_args(constructor, version_data, false);
            
            code.push_str(&format!("inline {} {}::{}({}) {{\r\n", 
                class_name, class_name, cpp_fn_name, cpp_args));
            
            if use_proxy {
                // C++03: return via Proxy to avoid copy constructor
                // Use explicit variable to avoid syntax ambiguity with functional cast
                code.push_str(&format!("    {}::Proxy _p({}({}));\r\n", 
                    class_name, c_fn_name, call_args));
                code.push_str("    return _p;\r\n");
            } else {
                // C++11+: direct construction with move semantics
                code.push_str(&format!("    return {}({}({}));\r\n", 
                    class_name, c_fn_name, call_args));
            }
            code.push_str("}\r\n\r\n");
        }
    }
    
    // Instance method implementations
    if let Some(functions) = &class_data.functions {
        for (fn_name, function) in functions {
            // Escape C++ reserved keywords in method names (e.g., "union" -> "union_")
            let cpp_fn_name = escape_cpp_keyword(&snake_case_to_lower_camel(fn_name));
            let c_fn_name = format!("{}_{}", c_type_name, snake_case_to_lower_camel(fn_name));
            
            // Determine how self is passed
            let (is_const, self_is_value) = function.fn_args.first()
                .and_then(|arg| arg.iter().next())
                .map(|(name, typ)| {
                    if name == "self" {
                        (typ == "ref" || typ == "value", typ == "value")
                    } else {
                        (false, false)
                    }
                })
                .unwrap_or((false, false));
            
            let cpp_return_type = if let Some(ret) = &function.returns {
                get_cpp_return_type(&ret.r#type, version_data)
            } else {
                "void".to_string()
            };
            
            let cpp_args = generate_cpp_args_signature(function, version_data, cpp_version, true);
            let const_suffix = if is_const { " const" } else { "" };
            let call_args = generate_cpp_call_args(function, version_data, true);
            
            // Build full call with self argument
            // If self is "value", we need to pass the inner value (consumes it)
            // If self is "ref" or "refmut", we pass a pointer
            let self_arg = if self_is_value { "inner_" } else { "&inner_" };
            let full_call_args = if call_args.is_empty() {
                self_arg.to_string()
            } else {
                format!("{}, {}", self_arg, call_args)
            };
            
            code.push_str(&format!("inline {} {}::{}({}){} {{\r\n", 
                cpp_return_type, class_name, cpp_fn_name, cpp_args, const_suffix));
            
            if cpp_return_type == "void" {
                code.push_str(&format!("    {}({});\r\n", c_fn_name, full_call_args));
            } else {
                // Check if return type needs wrapping
                if let Some(ret) = &function.returns {
                    let (_, base_type, _) = analyze_type(&ret.r#type);
                    if !is_primitive_arg(&base_type) && type_has_cpp_wrapper(&base_type, version_data) {
                        // For C++03, check if return type needs Proxy path
                        if !cpp_version.has_move_semantics() && type_needs_proxy_for_cpp03(&base_type, version_data) {
                            // Use explicit variable to avoid syntax ambiguity with functional cast
                            code.push_str(&format!("    {}::Proxy _p({}({}));\r\n", 
                                base_type, c_fn_name, full_call_args));
                            code.push_str("    return _p;\r\n");
                        } else {
                            code.push_str(&format!("    return {}({}({}));\r\n", 
                                base_type, c_fn_name, full_call_args));
                        }
                    } else {
                        code.push_str(&format!("    return {}({});\r\n", c_fn_name, full_call_args));
                    }
                } else {
                    code.push_str(&format!("    return {}({});\r\n", c_fn_name, full_call_args));
                }
            }
            code.push_str("}\r\n\r\n");
        }
    }
}

/// Generate C++ function argument signature
fn generate_cpp_args_signature(
    func: &crate::api::FunctionData,
    version_data: &VersionData,
    cpp_version: CppVersion,
    is_method: bool,
) -> String {
    let mut args = Vec::new();
    
    for (i, arg_map) in func.fn_args.iter().enumerate() {
        for (arg_name, arg_type) in arg_map {
            // Skip 'self' for methods
            if is_method && i == 0 && arg_name == "self" {
                continue;
            }
            
            let escaped_name = escape_cpp_keyword(arg_name);
            let (prefix, base_type, _) = analyze_type(arg_type);
            
            // Check if this is a pointer type
            let is_const_ptr = prefix.contains("*const") || prefix.contains("* const");
            let is_mut_ptr = prefix.contains("*mut") || prefix.contains("* mut");
            
            // Determine C++ type
            let cpp_type = if is_primitive_arg(&base_type) {
                // Handle primitive pointer types
                if is_const_ptr {
                    format!("const {}*", replace_primitive_ctype(&base_type))
                } else if is_mut_ptr {
                    format!("{}*", replace_primitive_ctype(&base_type))
                } else {
                    replace_primitive_ctype(&base_type)
                }
            } else if type_has_cpp_wrapper(&base_type, version_data) {
                // Use C++ wrapper type
                if is_const_ptr {
                    // For const pointer, use reference in C++
                    format!("{}& ", base_type)
                } else if is_mut_ptr {
                    // For mut pointer, use reference in C++
                    format!("{}&", base_type)
                } else if cpp_version.has_move_semantics() {
                    // For value types, use move semantics
                    format!("{}", base_type)
                } else {
                    format!("{}", base_type)
                }
            } else {
                // Use C type for callbacks, simple enums, type aliases, etc.
                if is_const_ptr {
                    format!("const {}{}*", C_PREFIX, base_type)
                } else if is_mut_ptr {
                    format!("{}{}*", C_PREFIX, base_type)
                } else {
                    format!("{}{}", C_PREFIX, base_type)
                }
            };
            
            args.push(format!("{} {}", cpp_type, escaped_name));
        }
    }
    
    args.join(", ")
}

/// Generate C++ function call arguments
fn generate_cpp_call_args(
    func: &crate::api::FunctionData,
    version_data: &VersionData,
    is_method: bool,
) -> String {
    let mut args = Vec::new();
    
    for (i, arg_map) in func.fn_args.iter().enumerate() {
        for (arg_name, arg_type) in arg_map {
            // Skip 'self' for methods
            if is_method && i == 0 && arg_name == "self" {
                continue;
            }
            
            let escaped_name = escape_cpp_keyword(arg_name);
            let (prefix, base_type, _) = analyze_type(arg_type);
            
            // Check if this is a pointer type
            let is_pointer = prefix.contains("*const") || prefix.contains("* const") 
                || prefix.contains("*mut") || prefix.contains("* mut");
            
            // For wrapper types:
            // - If pointer type, use .ptr() to get a pointer
            // - If value type, use .release() to consume and get the C type
            if !is_primitive_arg(&base_type) && type_has_cpp_wrapper(&base_type, version_data) {
                if is_pointer {
                    args.push(format!("{}.ptr()", escaped_name));
                } else {
                    args.push(format!("{}.release()", escaped_name));
                }
            } else {
                args.push(escaped_name.to_string());
            }
        }
    }
    
    args.join(", ")
}

/// Get C++ return type for a function
fn get_cpp_return_type(type_str: &str, version_data: &VersionData) -> String {
    let (_, base_type, _) = analyze_type(type_str);
    
    if is_primitive_arg(&base_type) {
        replace_primitive_ctype(&base_type)
    } else if type_has_cpp_wrapper(&base_type, version_data) {
        base_type
    } else {
        // Use C type for callbacks, simple enums, type aliases, etc.
        format!("{}{}", C_PREFIX, base_type)
    }
}

/// Generate all C++ API headers (for all versions)
pub fn generate_all_cpp_apis(api_data: &ApiData, version: &str) -> Vec<(String, String)> {
    CppVersion::all()
        .iter()
        .map(|&cpp_version| {
            (
                cpp_version.header_filename(),
                generate_cpp_api_versioned(api_data, version, cpp_version),
            )
        })
        .collect()
}
