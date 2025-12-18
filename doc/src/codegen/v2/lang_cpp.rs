//! C++ header generator
//!
//! Generates C++ header files from the IR, including:
//! - Class definitions with constructors/destructors
//! - Proper C++ types and RAII patterns
//! - Method wrappers around C functions
//! - Version-specific features (C++03 through C++23)

use anyhow::Result;

use super::config::*;
use super::generator::{CodeBuilder, LanguageGenerator};
use super::ir::*;

/// C++ reserved keywords that need to be escaped
const CPP_RESERVED_KEYWORDS: &[&str] = &[
    "alignas", "alignof", "and", "and_eq", "asm", "auto", "bitand", "bitor",
    "bool", "break", "case", "catch", "char", "char8_t", "char16_t", "char32_t",
    "class", "compl", "concept", "const", "consteval", "constexpr", "constinit",
    "const_cast", "continue", "co_await", "co_return", "co_yield", "decltype",
    "default", "delete", "do", "double", "dynamic_cast", "else", "enum",
    "explicit", "export", "extern", "false", "float", "for", "friend", "goto",
    "if", "inline", "int", "long", "mutable", "namespace", "new", "noexcept",
    "not", "not_eq", "nullptr", "operator", "or", "or_eq", "private",
    "protected", "public", "reflexpr", "register", "reinterpret_cast",
    "requires", "return", "short", "signed", "sizeof", "static",
    "static_assert", "static_cast", "struct", "switch", "synchronized",
    "template", "this", "thread_local", "throw", "true", "try", "typedef",
    "typeid", "typename", "union", "unsigned", "using", "virtual", "void",
    "volatile", "wchar_t", "while", "xor", "xor_eq",
];

/// Escape C++ reserved keywords by appending an underscore
pub fn escape_cpp_keyword(name: &str) -> String {
    if CPP_RESERVED_KEYWORDS.contains(&name) {
        format!("{}_", name)
    } else {
        name.to_string()
    }
}

// ============================================================================
// C++ Generator
// ============================================================================

pub struct CppGenerator {
    pub standard: CppStandard,
}

impl CppGenerator {
    pub fn new(standard: CppStandard) -> Self {
        Self { standard }
    }
    
    /// Generate C++ header code (alternative entry point)
    pub fn generate_header(&self, ir: &CodegenIR, config: &CodegenConfig) -> Result<String> {
        self.generate(ir, config)
    }
}

impl LanguageGenerator for CppGenerator {
    fn generate(&self, ir: &CodegenIR, config: &CodegenConfig) -> Result<String> {
        let mut code = String::new();
        let v = self.standard;

        // Header comment
        code.push_str("// =============================================================================\r\n");
        code.push_str(&format!("// Azul C++{} API Wrapper\r\n", v.version_number()));
        code.push_str("// =============================================================================\r\n");
        code.push_str("//\r\n");
        code.push_str(&format!("// Compile with: g++ {} -o myapp myapp.cpp -lazul\r\n", v.standard_flag()));
        code.push_str("//\r\n");
        code.push_str("// This header provides C++ wrapper classes for the Azul C API.\r\n");
        code.push_str("// All classes use RAII for memory management.\r\n");
        code.push_str("//\r\n");

        // Add version-specific feature documentation
        if v.has_string_view() {
            code.push_str("// C++17+ FEATURES:\r\n");
            code.push_str("//   - String supports std::string_view constructor and conversion\r\n");
            code.push_str("//   - Option types support toStdOptional() and std::optional conversion\r\n");
            code.push_str("//   - [[nodiscard]] attributes for static constructors\r\n");
            code.push_str("//\r\n");
        }
        if v.has_span() {
            code.push_str("// C++20+ FEATURES:\r\n");
            code.push_str("//   - Vec types support toSpan() and std::span conversion for zero-copy access\r\n");
            code.push_str("//\r\n");
        }
        if v.has_expected() {
            code.push_str("// C++23 FEATURES:\r\n");
            code.push_str("//   - Result types support toStdExpected() and std::expected conversion\r\n");
            code.push_str("//\r\n");
        }

        // Add C++03 specific documentation (Colvin-Gibbons trick)
        if !v.has_move_semantics() {
            code.push_str("// C++03 MOVE EMULATION (Colvin-Gibbons Trick)\r\n");
            code.push_str("// ============================================\r\n");
            code.push_str("//\r\n");
            code.push_str("// C++03 lacks move semantics, which normally prevents returning non-copyable\r\n");
            code.push_str("// RAII objects by value. This header uses the \"Colvin-Gibbons trick\" to work\r\n");
            code.push_str("// around this limitation.\r\n");
            code.push_str("//\r\n");
            code.push_str("// How it works:\r\n");
            code.push_str("// - Each non-copyable class has a nested 'Proxy' struct\r\n");
            code.push_str("// - When returning by value, the object converts to Proxy (releasing ownership)\r\n");
            code.push_str("// - The receiving object constructs from Proxy (acquiring ownership)\r\n");
            code.push_str("// - Direct copy construction transfers ownership (like std::auto_ptr)\r\n");
            code.push_str("//\r\n");
            code.push_str("// WARNING: These objects CANNOT be safely stored in C++03 STL containers!\r\n");
            code.push_str("//\r\n");
        }

        code.push_str("// =============================================================================\r\n");
        code.push_str("\r\n");

        // Include guards
        code.push_str(&format!("#ifndef AZUL_CPP{}_HPP\r\n", v.version_number()));
        code.push_str(&format!("#define AZUL_CPP{}_HPP\r\n", v.version_number()));
        code.push_str("\r\n");

        // Include the C header
        code.push_str("extern \"C\" {\r\n");
        code.push_str("#include \"azul.h\"\r\n");
        code.push_str("}\r\n");
        code.push_str("\r\n");

        // Standard includes
        if v.has_move_semantics() {
            code.push_str("#include <cstdint>\r\n");
            code.push_str("#include <cstddef>\r\n");
            code.push_str("#include <cstring>\r\n");
            code.push_str("#include <utility>\r\n");
            code.push_str("#include <stdexcept>\r\n");
            code.push_str("#include <string>\r\n");
            code.push_str("#include <vector>\r\n");
        } else {
            code.push_str("#include <stdint.h>\r\n");
            code.push_str("#include <stddef.h>\r\n");
            code.push_str("#include <string.h>\r\n");
        }
        if v.has_optional() {
            code.push_str("#include <optional>\r\n");
        }
        if v.has_variant() {
            code.push_str("#include <variant>\r\n");
        }
        if v.has_span() {
            code.push_str("#include <span>\r\n");
        }
        if v.has_string_view() {
            code.push_str("#include <string_view>\r\n");
        }
        if v.has_expected() {
            code.push_str("#include <expected>\r\n");
        }
        if v.has_std_function() {
            code.push_str("#include <functional>\r\n");
        }
        code.push_str("\r\n");

        // AZ_REFLECT macro for older C++ versions
        self.generate_reflect_macro(&mut code);

        // Open namespace
        code.push_str("namespace azul {\r\n");
        code.push_str("\r\n");

        // Collect types sorted by dependencies
        let sorted_structs = self.sort_types_by_dependencies(ir);

        // Forward declarations for all classes
        code.push_str("// Forward declarations\r\n");
        for struct_def in &sorted_structs {
            if !config.should_include_type(&struct_def.name) {
                continue;
            }
            if self.is_simple_enum_struct(struct_def) {
                continue;
            }
            if self.should_skip_class(struct_def) {
                continue;
            }
            // Skip types that will be rendered as type aliases
            if self.renders_as_type_alias(struct_def) {
                continue;
            }
            code.push_str(&format!("class {};\r\n", struct_def.name));
        }
        for enum_def in &ir.enums {
            if !config.should_include_type(&enum_def.name) {
                continue;
            }
            // Simple enums are rendered as type aliases, not classes
            // Tagged unions could be classes, but we use type alias for now
            // So skip all enum forward declarations
            continue;
        }
        code.push_str("\r\n");

        // Generate wrapper classes (declarations only, no method bodies)
        code.push_str("// Wrapper class declarations\r\n");
        code.push_str("\r\n");

        for struct_def in &sorted_structs {
            if !config.should_include_type(&struct_def.name) {
                continue;
            }
            self.generate_class_declaration(&mut code, struct_def, ir, config);
        }

        for enum_def in &ir.enums {
            if !config.should_include_type(&enum_def.name) {
                continue;
            }
            self.generate_enum_wrapper(&mut code, enum_def, config);
        }

        // Generate method implementations after all classes are declared
        code.push_str("// Method implementations\r\n");
        code.push_str("// (Implemented after all classes are declared to avoid incomplete type errors)\r\n");
        code.push_str("\r\n");

        for struct_def in &sorted_structs {
            if !config.should_include_type(&struct_def.name) {
                continue;
            }
            self.generate_method_implementations(&mut code, struct_def, ir, config);
        }

        // Close namespace
        code.push_str("} // namespace azul\r\n");
        code.push_str("\r\n");

        // End include guards
        code.push_str(&format!("#endif // AZUL_CPP{}_HPP\r\n", v.version_number()));

        Ok(code)
    }

    fn generate_types(&self, _ir: &CodegenIR, _config: &CodegenConfig) -> Result<String> {
        Ok(String::new())
    }

    fn generate_functions(&self, _ir: &CodegenIR, _config: &CodegenConfig) -> Result<String> {
        Ok(String::new())
    }

    fn generate_trait_impls(&self, _ir: &CodegenIR, _config: &CodegenConfig) -> Result<String> {
        Ok(String::new())
    }
}

// ============================================================================
// Helper methods
// ============================================================================

impl CppGenerator {
    /// Generate the AZ_REFLECT macro for RTTI support
    fn generate_reflect_macro(&self, code: &mut String) {
        let v = self.standard;
        
        // First, generate a helper function to create AzString from const char*
        // This avoids using the C macro AzString_fromConstStr which uses C99 designated initializers
        code.push_str("// Helper to create AzString from string literal (avoids C macro incompatibilities)\r\n");
        if v.has_move_semantics() {
            code.push_str("inline AzString az_string_from_literal(const char* s) {\r\n");
            code.push_str("    return AzString_copyFromBytes(reinterpret_cast<const uint8_t*>(s), 0, std::strlen(s));\r\n");
            code.push_str("}\r\n\r\n");
        } else {
            code.push_str("inline AzString az_string_from_literal(const char* s) {\r\n");
            code.push_str("    return AzString_copyFromBytes((const uint8_t*)s, 0, strlen(s));\r\n");
            code.push_str("}\r\n\r\n");
        }
        
        code.push_str("// =============================================================================\r\n");
        code.push_str("// AZ_REFLECT Macro - Runtime Type Information for user types\r\n");
        code.push_str("// =============================================================================\r\n");
        code.push_str("//\r\n");
        code.push_str("// Usage:\r\n");
        code.push_str("//   struct MyDataModel { uint32_t counter; };\r\n");
        code.push_str("//   AZ_REFLECT(MyDataModel);\r\n");
        code.push_str("//\r\n");
        code.push_str("// This generates:\r\n");
        code.push_str("//   - MyDataModel_upcast(model) -> RefAny\r\n");
        code.push_str("//   - MyDataModel_downcast_ref(refany) -> MyDataModel const*\r\n");
        code.push_str("//   - MyDataModel_downcast_mut(refany) -> MyDataModel*\r\n");
        code.push_str("// =============================================================================\r\n\r\n");
        
        if v.has_move_semantics() {
            // C++11 and later: use static_cast, move semantics
            code.push_str("#define AZ_REFLECT(structName) \\\r\n");
            code.push_str("    namespace structName##_rtti { \\\r\n");
            code.push_str("        static const uint64_t type_id_storage = 0; \\\r\n");
            code.push_str("        inline uint64_t type_id() { return reinterpret_cast<uint64_t>(&type_id_storage); } \\\r\n");
            code.push_str("        inline void destructor(void* ptr) { delete static_cast<structName*>(ptr); } \\\r\n");
            code.push_str("    } \\\r\n");
            code.push_str("    static inline azul::RefAny structName##_upcast(structName model) { \\\r\n");
            code.push_str("        structName* heap = new structName(std::move(model)); \\\r\n");
            code.push_str("        AzGlVoidPtrConst ptr = { heap, true }; \\\r\n");
            code.push_str("        AzString name = az_string_from_literal(#structName); \\\r\n");
            code.push_str("        return azul::RefAny(AzRefAny_newC(ptr, sizeof(structName), alignof(structName), \\\r\n");
            code.push_str("            structName##_rtti::type_id(), name, structName##_rtti::destructor)); \\\r\n");
            code.push_str("    } \\\r\n");
            code.push_str("    static inline structName const* structName##_downcast_ref(azul::RefAny& data) { \\\r\n");
            code.push_str("        if (!AzRefAny_isType(&data.inner(), structName##_rtti::type_id())) return nullptr; \\\r\n");
            code.push_str("        return static_cast<structName const*>(data.inner()._internal_ptr); \\\r\n");
            code.push_str("    } \\\r\n");
            code.push_str("    static inline structName* structName##_downcast_mut(azul::RefAny& data) { \\\r\n");
            code.push_str("        if (!AzRefAny_isType(&data.inner(), structName##_rtti::type_id())) return nullptr; \\\r\n");
            code.push_str("        return static_cast<structName*>(const_cast<void*>(data.inner()._internal_ptr)); \\\r\n");
            code.push_str("    }\r\n\r\n");
        } else {
            // C++03: no move semantics, use copy
            code.push_str("#define AZ_REFLECT(structName) \\\r\n");
            code.push_str("    static const uint64_t structName##_type_id_storage = 0; \\\r\n");
            code.push_str("    static uint64_t structName##_type_id() { return (uint64_t)(&structName##_type_id_storage); } \\\r\n");
            code.push_str("    static void structName##_destructor(void* ptr) { delete (structName*)ptr; } \\\r\n");
            code.push_str("    static azul::RefAny structName##_upcast(structName model) { \\\r\n");
            code.push_str("        structName* heap = new structName(model); \\\r\n");
            code.push_str("        AzGlVoidPtrConst ptr; ptr.ptr = heap; ptr.run_destructor = 1; \\\r\n");
            code.push_str("        AzString name = az_string_from_literal(#structName); \\\r\n");
            code.push_str("        return azul::RefAny(AzRefAny_newC(ptr, sizeof(structName), \\\r\n");
            code.push_str("            sizeof(structName), structName##_type_id(), name, structName##_destructor)); \\\r\n");
            code.push_str("    } \\\r\n");
            code.push_str("    static structName const* structName##_downcast_ref(azul::RefAny& data) { \\\r\n");
            code.push_str("        if (!AzRefAny_isType(&data.inner(), structName##_type_id())) return 0; \\\r\n");
            code.push_str("        return (structName const*)(data.inner()._internal_ptr); \\\r\n");
            code.push_str("    } \\\r\n");
            code.push_str("    static structName* structName##_downcast_mut(azul::RefAny& data) { \\\r\n");
            code.push_str("        if (!AzRefAny_isType(&data.inner(), structName##_type_id())) return 0; \\\r\n");
            code.push_str("        return (structName*)(data.inner()._internal_ptr); \\\r\n");
            code.push_str("    }\r\n\r\n");
        }
    }

    /// Returns "std::memset" for C++11+ and "memset" for C++03
    fn memset_fn(&self) -> &'static str {
        if self.standard.has_move_semantics() {
            "std::memset"
        } else {
            "memset"
        }
    }

    /// Returns "std::strlen" for C++11+ and "strlen" for C++03
    fn strlen_fn(&self) -> &'static str {
        if self.standard.has_move_semantics() {
            "std::strlen"
        } else {
            "strlen"
        }
    }

    /// Check if the first argument is a "self" parameter
    /// In the IR, self parameters have type_name == class_name and name == class_name.to_lowercase()
    fn is_self_arg(arg: &FunctionArg, class_name: &str) -> bool {
        arg.type_name == class_name && arg.name == class_name.to_lowercase()
    }

    /// Check if a function has a self parameter
    fn func_has_self(func: &FunctionDef) -> bool {
        func.args.first()
            .map(|a| Self::is_self_arg(a, &func.class_name))
            .unwrap_or(false)
    }

    fn sort_types_by_dependencies<'a>(&self, ir: &'a CodegenIR) -> Vec<&'a StructDef> {
        let mut structs: Vec<&StructDef> = ir.structs.iter().collect();
        structs.sort_by_key(|s| s.sort_order);
        structs
    }

    fn is_simple_enum_struct(&self, _struct_def: &StructDef) -> bool {
        false
    }

    fn should_skip_class(&self, struct_def: &StructDef) -> bool {
        matches!(struct_def.category, TypeCategory::CallbackTypedef | TypeCategory::GenericTemplate)
    }

    /// Check if a struct will be rendered as a type alias instead of a real class
    fn renders_as_type_alias(&self, struct_def: &StructDef) -> bool {
        struct_def.fields.is_empty() && struct_def.derives.contains(&"Copy".to_string())
    }

    fn is_copy(&self, struct_def: &StructDef) -> bool {
        struct_def.traits.is_copy
    }

    fn needs_destructor(&self, struct_def: &StructDef) -> bool {
        struct_def.traits.needs_delete()
    }

    fn is_vec_type(&self, struct_def: &StructDef) -> bool {
        matches!(struct_def.category, TypeCategory::Vec)
    }

    fn is_string_type(&self, struct_def: &StructDef) -> bool {
        matches!(struct_def.category, TypeCategory::String)
    }

    fn is_option_type(&self, struct_def: &StructDef) -> bool {
        // Check if name starts with "Option" and has typical Option-like structure
        struct_def.name.starts_with("Option")
    }

    fn is_result_type(&self, struct_def: &StructDef) -> bool {
        // Check if name starts with "Result" and has typical Result-like structure
        struct_def.name.starts_with("Result")
    }

    fn noexcept(&self) -> &'static str {
        if self.standard.has_noexcept() { " noexcept" } else { "" }
    }

    fn nodiscard(&self) -> &'static str {
        if self.standard.has_nodiscard() { "[[nodiscard]] " } else { "" }
    }

    fn generate_class_declaration(
        &self,
        code: &mut String,
        struct_def: &StructDef,
        ir: &CodegenIR,
        config: &CodegenConfig,
    ) {
        if self.should_skip_class(struct_def) {
            return;
        }

        let class_name = &struct_def.name;
        let c_type_name = config.apply_prefix(class_name);
        let is_copy = self.is_copy(struct_def);
        let needs_destructor = self.needs_destructor(struct_def);
        let v = self.standard;

        if struct_def.fields.is_empty() && struct_def.derives.contains(&"Copy".to_string()) {
            if v.has_move_semantics() {
                code.push_str(&format!("using {} = {};\r\n\r\n", class_name, c_type_name));
            } else {
                code.push_str(&format!("typedef {} {};\r\n\r\n", c_type_name, class_name));
            }
            return;
        }

        code.push_str(&format!("class {} {{\r\n", class_name));

        if !v.has_move_semantics() && !is_copy {
            code.push_str("public:\r\n");
            code.push_str(&format!(
                "    struct Proxy {{ {} inner; Proxy({} p) : inner(p) {{}} }};\r\n",
                c_type_name, c_type_name
            ));
            code.push_str("\r\n");
        }

        code.push_str("private:\r\n");
        if !v.has_move_semantics() && !is_copy {
            code.push_str(&format!("    mutable {} inner_;\r\n", c_type_name));
        } else {
            code.push_str(&format!("    {} inner_;\r\n", c_type_name));
        }
        code.push_str("\r\n");

        if !is_copy && v.has_move_semantics() {
            code.push_str(&format!("    {}(const {}&) = delete;\r\n", class_name, class_name));
            code.push_str(&format!("    {}& operator=(const {}&) = delete;\r\n", class_name, class_name));
        }

        code.push_str("\r\n");
        code.push_str("public:\r\n");

        let noexcept = self.noexcept();
        code.push_str(&format!(
            "    explicit {}({} inner){} : inner_(inner) {{}}\r\n",
            class_name, c_type_name, noexcept
        ));

        if is_copy {
            code.push_str(&format!(
                "    {}(const {}& other){} : inner_(other.inner_) {{}}\r\n",
                class_name, class_name, noexcept
            ));
            code.push_str(&format!(
                "    {}& operator=(const {}& other){} {{ inner_ = other.inner_; return *this; }}\r\n",
                class_name, class_name, noexcept
            ));
        }

        let memset = self.memset_fn();

        if !v.has_move_semantics() && !is_copy {
            code.push_str(&format!(
                "    {}(const {}& other) : inner_(other.inner_) {{ {}(const_cast<{}*>(&other.inner_), 0, sizeof(other.inner_)); }}\r\n",
                class_name, class_name, memset, c_type_name
            ));
            code.push_str(&format!(
                "    {}& operator=(const {}& other) {{\r\n",
                class_name, class_name
            ));
            if needs_destructor {
                code.push_str(&format!("        {}_delete(&inner_);\r\n", c_type_name));
            }
            code.push_str("        inner_ = other.inner_;\r\n");
            code.push_str(&format!("        {}(const_cast<{}*>(&other.inner_), 0, sizeof(other.inner_));\r\n", memset, c_type_name));
            code.push_str("        return *this;\r\n");
            code.push_str("    }\r\n");
            code.push_str(&format!("    {}(Proxy p) : inner_(p.inner) {{}}\r\n", class_name));
            code.push_str("    operator Proxy() {\r\n");
            code.push_str("        Proxy p(inner_);\r\n");
            code.push_str(&format!("        {}(&inner_, 0, sizeof(inner_));\r\n", memset));
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

        if needs_destructor {
            code.push_str(&format!(
                "    ~{}() {{ {}_delete(&inner_); }}\r\n",
                class_name, c_type_name
            ));
        } else {
            code.push_str(&format!("    ~{}() {{}}\r\n", class_name));
        }

        if v.has_move_semantics() && !is_copy {
            code.push_str("\r\n");
            code.push_str(&format!(
                "    {}({}&& other) noexcept : inner_(other.inner_) {{\r\n",
                class_name, class_name
            ));
            code.push_str("        other.inner_ = {};\r\n");
            code.push_str("    }\r\n");

            code.push_str(&format!(
                "    {}& operator=({}&& other) noexcept {{\r\n",
                class_name, class_name
            ));
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

        // Filter constructors to only those that actually return the class type
        // Constructors returning Option<T>, Result<T, E>, etc. become static methods
        let constructors: Vec<_> = ir.functions.iter()
            .filter(|f| f.class_name == *class_name && matches!(f.kind, FunctionKind::Constructor))
            .filter(|f| f.return_type.as_deref() == Some(class_name))
            .collect();
        
        if !constructors.is_empty() {
            code.push_str("\r\n");
            for func in constructors {
                let cpp_fn_name = self.escape_method_name(&func.method_name);
                let cpp_args = self.generate_args_signature(&func.args, ir, config, false, class_name);
                let nodiscard = self.nodiscard();
                code.push_str(&format!(
                    "    {}static {} {}({});\r\n",
                    nodiscard, class_name, cpp_fn_name, cpp_args
                ));
            }
        }

        // Also include "constructors" that return a different type (Option, Result) as static methods
        let factory_methods: Vec<_> = ir.functions.iter()
            .filter(|f| f.class_name == *class_name && matches!(f.kind, FunctionKind::Constructor))
            .filter(|f| f.return_type.as_deref() != Some(class_name))
            .collect();

        if !factory_methods.is_empty() {
            for func in factory_methods {
                let cpp_fn_name = self.escape_method_name(&func.method_name);
                let cpp_args = self.generate_args_signature(&func.args, ir, config, false, class_name);
                let cpp_return_type = self.get_cpp_return_type(func.return_type.as_deref(), ir);
                let nodiscard = self.nodiscard();
                code.push_str(&format!(
                    "    {}static {} {}({});\r\n",
                    nodiscard, cpp_return_type, cpp_fn_name, cpp_args
                ));
            }
        }

        let methods: Vec<_> = ir.functions.iter()
            .filter(|f| f.class_name == *class_name && !matches!(f.kind, FunctionKind::Constructor) && !f.kind.is_trait_function())
            .collect();
        
        if !methods.is_empty() {
            code.push_str("\r\n");
            for func in methods {
                let cpp_fn_name = self.escape_method_name(&func.method_name);
                let has_self = Self::func_has_self(func);
                let is_const = has_self && (matches!(func.kind, FunctionKind::Method) || func.is_const);
                let const_suffix = if is_const { " const" } else { "" };
                let static_prefix = if !has_self { "static " } else { "" };
                let cpp_return_type = self.get_cpp_return_type(func.return_type.as_deref(), ir);
                let cpp_args = self.generate_args_signature(&func.args, ir, config, true, class_name);
                code.push_str(&format!(
                    "    {}{} {}({}){};\r\n",
                    static_prefix, cpp_return_type, cpp_fn_name, cpp_args, const_suffix
                ));
            }
        }

        code.push_str("\r\n");
        code.push_str(&format!(
            "    const {}& inner() const {{ return inner_; }}\r\n",
            c_type_name
        ));
        code.push_str(&format!(
            "    {}& inner() {{ return inner_; }}\r\n",
            c_type_name
        ));
        code.push_str(&format!(
            "    const {}* ptr() const {{ return &inner_; }}\r\n",
            c_type_name
        ));
        code.push_str(&format!(
            "    {}* ptr() {{ return &inner_; }}\r\n",
            c_type_name
        ));

        if v.has_move_semantics() {
            code.push_str(&format!(
                "    {} release() {{ {} result = inner_; inner_ = {{}}; return result; }}\r\n",
                c_type_name, c_type_name
            ));
        } else {
            let memset = self.memset_fn();
            code.push_str(&format!(
                "    {} release() {{ {} result = inner_; {}(&inner_, 0, sizeof(inner_)); return result; }}\r\n",
                c_type_name, c_type_name, memset
            ));
        }

        if self.is_vec_type(struct_def) {
            self.generate_vec_methods(code, struct_def, config);
        }

        if self.is_string_type(struct_def) {
            self.generate_string_methods(code, struct_def, config);
        }

        if self.is_option_type(struct_def) {
            self.generate_option_methods(code, struct_def, config);
        }

        if self.is_result_type(struct_def) {
            self.generate_result_methods(code, struct_def, config);
        }

        code.push_str("};\r\n\r\n");
    }

    fn generate_vec_methods(&self, code: &mut String, struct_def: &StructDef, _config: &CodegenConfig) {
        let v = self.standard;
        
        let elem_type = struct_def.fields.iter()
            .find(|f| f.name == "ptr")
            .map(|f| &f.type_name)
            .cloned()
            .unwrap_or_else(|| "void".to_string());
        
        if elem_type == "c_void" || elem_type == "void" {
            return;
        }

        let c_elem_type = if self.is_primitive(&elem_type) {
            self.primitive_to_c(&elem_type)
        } else {
            format!("Az{}", elem_type)
        };

        code.push_str("\r\n");
        code.push_str("    // Iterator support for range-based for loops\r\n");
        code.push_str(&format!(
            "    const {}* begin() const {{ return inner_.ptr; }}\r\n",
            c_elem_type
        ));
        code.push_str(&format!(
            "    const {}* end() const {{ return inner_.ptr + inner_.len; }}\r\n",
            c_elem_type
        ));
        code.push_str(&format!(
            "    {}* begin() {{ return const_cast<{}*>(inner_.ptr); }}\r\n",
            c_elem_type, c_elem_type
        ));
        code.push_str(&format!(
            "    {}* end() {{ return const_cast<{}*>(inner_.ptr) + inner_.len; }}\r\n",
            c_elem_type, c_elem_type
        ));
        code.push_str("    size_t size() const { return inner_.len; }\r\n");
        code.push_str("    bool empty() const { return inner_.len == 0; }\r\n");
        code.push_str(&format!(
            "    const {}& operator[](size_t i) const {{ return inner_.ptr[i]; }}\r\n",
            c_elem_type
        ));
        code.push_str(&format!(
            "    {}& operator[](size_t i) {{ return const_cast<{}*>(inner_.ptr)[i]; }}\r\n",
            c_elem_type, c_elem_type
        ));

        if v.has_move_semantics() {
            code.push_str(&format!(
                "    std::vector<{}> toStdVector() const {{ return std::vector<{}>(begin(), end()); }}\r\n",
                c_elem_type, c_elem_type
            ));
        }

        if v.has_span() {
            code.push_str(&format!(
                "    std::span<const {}> toSpan() const {{ return std::span<const {}>(begin(), size()); }}\r\n",
                c_elem_type, c_elem_type
            ));
            code.push_str(&format!(
                "    std::span<{}> toSpan() {{ return std::span<{}>(begin(), size()); }}\r\n",
                c_elem_type, c_elem_type
            ));
            code.push_str(&format!(
                "    operator std::span<const {}>() const {{ return toSpan(); }}\r\n",
                c_elem_type
            ));
        }
    }

    fn generate_string_methods(&self, code: &mut String, _struct_def: &StructDef, _config: &CodegenConfig) {
        let v = self.standard;

        code.push_str("\r\n");
        code.push_str("    // std::string interoperability\r\n");
        let strlen = self.strlen_fn();
        if v.has_move_semantics() {
            code.push_str(&format!(
                "    String(const char* s) : inner_(AzString_copyFromBytes(reinterpret_cast<const uint8_t*>(s), 0, {}(s))) {{}}\r\n",
                strlen
            ));
            code.push_str(
                "    String(const std::string& s) : inner_(AzString_copyFromBytes(reinterpret_cast<const uint8_t*>(s.c_str()), 0, s.size())) {}\r\n"
            );
        } else {
            code.push_str(&format!(
                "    explicit String(const char* s) : inner_(AzString_copyFromBytes(reinterpret_cast<const uint8_t*>(s), 0, {}(s))) {{}}\r\n",
                strlen
            ));
        }
        if v.has_string_view() {
            code.push_str(
                "    String(std::string_view sv) : inner_(AzString_copyFromBytes(reinterpret_cast<const uint8_t*>(sv.data()), 0, sv.size())) {}\r\n"
            );
        }
        code.push_str(
            "    const char* c_str() const { return reinterpret_cast<const char*>(inner_.vec.ptr); }\r\n"
        );
        code.push_str("    size_t length() const { return inner_.vec.len; }\r\n");
        if v.has_move_semantics() {
            code.push_str(
                "    std::string toStdString() const { return std::string(c_str(), length()); }\r\n"
            );
            code.push_str("    operator std::string() const { return toStdString(); }\r\n");
        }
        if v.has_string_view() {
            code.push_str(
                "    std::string_view toStringView() const { return std::string_view(c_str(), length()); }\r\n"
            );
            code.push_str("    operator std::string_view() const { return toStringView(); }\r\n");
        }
    }

    fn generate_option_methods(&self, code: &mut String, struct_def: &StructDef, config: &CodegenConfig) {
        let v = self.standard;
        let c_type_name = config.apply_prefix(&struct_def.name);

        let inner_type = struct_def.name.strip_prefix("Option").unwrap_or("void");
        let c_inner_type = if self.is_primitive(inner_type) {
            self.primitive_to_c(inner_type)
        } else {
            format!("Az{}", inner_type)
        };

        code.push_str("\r\n");
        code.push_str("    // Option convenience methods\r\n");
        code.push_str(&format!(
            "    bool isSome() const {{ return inner_.Some.tag == {}_Tag_Some; }}\r\n",
            c_type_name
        ));
        code.push_str(&format!(
            "    bool isNone() const {{ return inner_.Some.tag == {}_Tag_None; }}\r\n",
            c_type_name
        ));

        if v.has_move_semantics() {
            code.push_str("    explicit operator bool() const { return isSome(); }\r\n");
        }

        code.push_str(&format!(
            "    const {}& unwrap() const {{ return inner_.Some.payload; }}\r\n",
            c_inner_type
        ));
        code.push_str(&format!(
            "    {}& unwrap() {{ return inner_.Some.payload; }}\r\n",
            c_inner_type
        ));
        code.push_str(&format!(
            "    {} unwrapOr(const {}& defaultValue) const {{ return isSome() ? inner_.Some.payload : defaultValue; }}\r\n",
            c_inner_type, c_inner_type
        ));

        if v.has_optional() {
            code.push_str(&format!(
                "    std::optional<{}> toStdOptional() const {{ return isSome() ? std::optional<{}>(inner_.Some.payload) : std::nullopt; }}\r\n",
                c_inner_type, c_inner_type
            ));
            code.push_str(&format!(
                "    operator std::optional<{}>() const {{ return toStdOptional(); }}\r\n",
                c_inner_type
            ));
        }
    }

    fn generate_result_methods(&self, code: &mut String, struct_def: &StructDef, config: &CodegenConfig) {
        let v = self.standard;
        let c_type_name = config.apply_prefix(&struct_def.name);

        code.push_str("\r\n");
        code.push_str("    // Result convenience methods\r\n");
        code.push_str(&format!(
            "    bool isOk() const {{ return inner_.Ok.tag == {}_Tag_Ok; }}\r\n",
            c_type_name
        ));
        code.push_str(&format!(
            "    bool isErr() const {{ return inner_.Ok.tag == {}_Tag_Err; }}\r\n",
            c_type_name
        ));

        if v.has_move_semantics() {
            code.push_str("    explicit operator bool() const { return isOk(); }\r\n");
        }
    }

    fn generate_enum_wrapper(&self, code: &mut String, enum_def: &EnumDef, config: &CodegenConfig) {
        // Skip generic templates - they don't have a C type
        if !enum_def.generic_params.is_empty() {
            return;
        }
        
        let enum_name = &enum_def.name;
        let c_type_name = config.apply_prefix(enum_name);
        let v = self.standard;

        if enum_def.is_union {
            code.push_str(&format!("// {} is a tagged union - use C API or wrapper methods\r\n", enum_name));
            if v.has_move_semantics() {
                code.push_str(&format!("using {} = {};\r\n\r\n", enum_name, c_type_name));
            } else {
                code.push_str(&format!("typedef {} {};\r\n\r\n", c_type_name, enum_name));
            }
        } else {
            if v.has_enum_class() {
                code.push_str(&format!("using {} = {};\r\n\r\n", enum_name, c_type_name));
            } else {
                code.push_str(&format!("typedef {} {};\r\n\r\n", c_type_name, enum_name));
            }
        }
    }

    fn generate_method_implementations(
        &self,
        code: &mut String,
        struct_def: &StructDef,
        ir: &CodegenIR,
        config: &CodegenConfig,
    ) {
        if self.should_skip_class(struct_def) {
            return;
        }

        let class_name = &struct_def.name;
        let c_type_name = config.apply_prefix(class_name);
        let is_copy = self.is_copy(struct_def);
        let v = self.standard;

        let use_proxy = !v.has_move_semantics() && !is_copy;

        // Real constructors - return the class type
        for func in ir.functions.iter()
            .filter(|f| f.class_name == *class_name && matches!(f.kind, FunctionKind::Constructor))
            .filter(|f| f.return_type.as_deref() == Some(class_name.as_str()))
        {
            let cpp_fn_name = self.escape_method_name(&func.method_name);
            let c_fn_name = &func.c_name;
            let cpp_args = self.generate_args_signature(&func.args, ir, config, false, class_name);
            let call_args = self.generate_call_args(&func.args, ir, false, class_name);

            code.push_str(&format!(
                "inline {} {}::{}({}) {{\r\n",
                class_name, class_name, cpp_fn_name, cpp_args
            ));

            if use_proxy {
                code.push_str(&format!(
                    "    {}::Proxy _p({}({}));\r\n",
                    class_name, c_fn_name, call_args
                ));
                code.push_str("    return _p;\r\n");
            } else {
                code.push_str(&format!(
                    "    return {}({}({}));\r\n",
                    class_name, c_fn_name, call_args
                ));
            }
            code.push_str("}\r\n\r\n");
        }

        // Factory methods - return Option/Result types
        for func in ir.functions.iter()
            .filter(|f| f.class_name == *class_name && matches!(f.kind, FunctionKind::Constructor))
            .filter(|f| f.return_type.as_deref() != Some(class_name.as_str()))
        {
            let cpp_fn_name = self.escape_method_name(&func.method_name);
            let c_fn_name = &func.c_name;
            let cpp_return_type = self.get_cpp_return_type(func.return_type.as_deref(), ir);
            let cpp_args = self.generate_args_signature(&func.args, ir, config, false, class_name);
            let call_args = self.generate_call_args(&func.args, ir, false, class_name);

            code.push_str(&format!(
                "inline {} {}::{}({}) {{\r\n",
                cpp_return_type, class_name, cpp_fn_name, cpp_args
            ));
            
            // Factory methods just return the C function result directly
            code.push_str(&format!(
                "    return {}({});\r\n",
                c_fn_name, call_args
            ));
            code.push_str("}\r\n\r\n");
        }

        for func in ir.functions.iter().filter(|f| f.class_name == *class_name && !matches!(f.kind, FunctionKind::Constructor) && !f.kind.is_trait_function()) {
            let cpp_fn_name = self.escape_method_name(&func.method_name);
            let c_fn_name = &func.c_name;
            
            // Check if function has a self argument
            let has_self = Self::func_has_self(func);
            
            let is_const = has_self && (matches!(func.kind, FunctionKind::Method) || func.is_const);
            let const_suffix = if is_const { " const" } else { "" };
            let cpp_return_type = self.get_cpp_return_type(func.return_type.as_deref(), ir);
            let cpp_args = self.generate_args_signature(&func.args, ir, config, true, class_name);
            let call_args = self.generate_call_args(&func.args, ir, true, class_name);

            let full_call_args = if has_self {
                let self_is_value = func.args.first()
                    .map(|a| a.ref_kind == ArgRefKind::Owned)
                    .unwrap_or(false);
                let self_arg = if self_is_value { "inner_" } else { "&inner_" };
                if call_args.is_empty() {
                    self_arg.to_string()
                } else {
                    format!("{}, {}", self_arg, call_args)
                }
            } else {
                call_args.clone()
            };

            code.push_str(&format!(
                "inline {} {}::{}({}){} {{\r\n",
                cpp_return_type, class_name, cpp_fn_name, cpp_args, const_suffix
            ));

            if cpp_return_type == "void" {
                code.push_str(&format!("    {}({});\r\n", c_fn_name, full_call_args));
            } else {
                let return_type_str = func.return_type.as_deref().unwrap_or("");
                if self.type_has_wrapper(return_type_str, ir) {
                    if use_proxy && self.type_needs_proxy(return_type_str, ir) {
                        code.push_str(&format!(
                            "    {}::Proxy _p({}({}));\r\n",
                            return_type_str, c_fn_name, full_call_args
                        ));
                        code.push_str("    return _p;\r\n");
                    } else {
                        code.push_str(&format!(
                            "    return {}({}({}));\r\n",
                            return_type_str, c_fn_name, full_call_args
                        ));
                    }
                } else {
                    code.push_str(&format!("    return {}({});\r\n", c_fn_name, full_call_args));
                }
            }
            code.push_str("}\r\n\r\n");
        }
    }

    fn escape_method_name(&self, name: &str) -> String {
        let name = if name == "new" { "new_" } else if name == "default" { "default_" } else { name };
        escape_cpp_keyword(name)
    }

    fn generate_args_signature(&self, args: &[FunctionArg], ir: &CodegenIR, config: &CodegenConfig, is_method: bool, class_name: &str) -> String {
        let mut result = Vec::new();
        
        for (i, arg) in args.iter().enumerate() {
            // Skip self parameter (first arg with type == class_name)
            if is_method && i == 0 && Self::is_self_arg(arg, class_name) {
                continue;
            }

            let escaped_name = escape_cpp_keyword(&arg.name);
            let cpp_type = self.arg_to_cpp_type(arg, ir, config);
            result.push(format!("{} {}", cpp_type, escaped_name));
        }

        result.join(", ")
    }

    fn generate_call_args(&self, args: &[FunctionArg], ir: &CodegenIR, is_method: bool, class_name: &str) -> String {
        let mut result = Vec::new();
        
        for (i, arg) in args.iter().enumerate() {
            // Skip self parameter (first arg with type == class_name)
            if is_method && i == 0 && Self::is_self_arg(arg, class_name) {
                continue;
            }

            let escaped_name = escape_cpp_keyword(&arg.name);
            
            if self.type_has_wrapper(&arg.type_name, ir) {
                let is_pointer = matches!(arg.ref_kind, ArgRefKind::Ptr | ArgRefKind::PtrMut | ArgRefKind::Ref | ArgRefKind::RefMut);
                if is_pointer {
                    result.push(format!("{}.ptr()", escaped_name));
                } else {
                    result.push(format!("{}.release()", escaped_name));
                }
            } else {
                result.push(escaped_name);
            }
        }

        result.join(", ")
    }

    fn arg_to_cpp_type(&self, arg: &FunctionArg, ir: &CodegenIR, config: &CodegenConfig) -> String {
        let base_type = &arg.type_name;
        
        if self.is_primitive(base_type) {
            let c_type = self.primitive_to_c(base_type);
            match arg.ref_kind {
                ArgRefKind::Ptr => format!("const {}*", c_type),
                ArgRefKind::PtrMut => format!("{}*", c_type),
                _ => c_type,
            }
        } else if self.type_has_wrapper(base_type, ir) {
            match arg.ref_kind {
                ArgRefKind::Ptr | ArgRefKind::Ref => format!("const {}&", base_type),
                ArgRefKind::PtrMut | ArgRefKind::RefMut => format!("{}&", base_type),
                ArgRefKind::Owned => base_type.clone(),
            }
        } else {
            let c_type = config.apply_prefix(base_type);
            match arg.ref_kind {
                ArgRefKind::Ptr => format!("const {}*", c_type),
                ArgRefKind::PtrMut => format!("{}*", c_type),
                _ => c_type,
            }
        }
    }

    fn get_cpp_return_type(&self, return_type: Option<&str>, ir: &CodegenIR) -> String {
        match return_type {
            None => "void".to_string(),
            Some(rt) => {
                if self.is_primitive(rt) {
                    self.primitive_to_c(rt)
                } else if self.type_has_wrapper(rt, ir) {
                    rt.to_string()
                } else {
                    format!("Az{}", rt)
                }
            }
        }
    }

    fn type_has_wrapper(&self, type_name: &str, ir: &CodegenIR) -> bool {
        ir.find_struct(type_name).is_some()
    }

    fn type_needs_proxy(&self, type_name: &str, ir: &CodegenIR) -> bool {
        if !self.type_has_wrapper(type_name, ir) {
            return false;
        }
        if let Some(s) = ir.find_struct(type_name) {
            return !s.traits.is_copy;
        }
        false
    }

    fn is_primitive(&self, type_name: &str) -> bool {
        matches!(type_name,
            "bool" | "u8" | "u16" | "u32" | "u64" | "usize" |
            "i8" | "i16" | "i32" | "i64" | "isize" |
            "f32" | "f64" | "c_void" | "()"
        )
    }

    fn primitive_to_c(&self, type_name: &str) -> String {
        match type_name {
            "bool" => "bool".to_string(),
            "u8" => "uint8_t".to_string(),
            "u16" => "uint16_t".to_string(),
            "u32" => "uint32_t".to_string(),
            "u64" => "uint64_t".to_string(),
            "usize" => "size_t".to_string(),
            "i8" => "int8_t".to_string(),
            "i16" => "int16_t".to_string(),
            "i32" => "int32_t".to_string(),
            "i64" => "int64_t".to_string(),
            "isize" => "ptrdiff_t".to_string(),
            "f32" => "float".to_string(),
            "f64" => "double".to_string(),
            "c_void" | "()" => "void".to_string(),
            _ => type_name.to_string(),
        }
    }
}
