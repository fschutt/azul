//! C++17 Generator
//!
//! This generator produces C++17-compatible code with additional features:
//!
//! - [[nodiscard]] attribute for factory methods
//! - std::optional support for Option types
//! - std::string_view support for String type
//! - std::variant support for tagged unions (future)

use anyhow::Result;
use super::{CppDialect, common::*};
use super::super::config::*;
use super::super::ir::*;

/// C++17 dialect generator
pub struct Cpp17Generator;

impl CppDialect for Cpp17Generator {
    fn standard(&self) -> CppStandard {
        CppStandard::Cpp17
    }
    
    fn generate(&self, ir: &CodegenIR, config: &CodegenConfig) -> Result<String> {
        let mut code = String::new();
        let std = self.standard();
        
        // Header comment
        code.push_str(&generate_header_comment(std));
        code.push_str(&generate_feature_docs(std));
        code.push_str("// =============================================================================\r\n\r\n");
        
        // Include guards
        code.push_str(&generate_include_guards_begin(std));
        
        // Includes
        code.push_str(&generate_includes(std));
        
        // AZ_REFLECT macro
        code.push_str(&generate_reflect_macro(std));
        
        // Open namespace
        code.push_str("namespace azul {\r\n\r\n");
        
        // Sort structs by dependency order
        let sorted_structs = self.sort_types_by_dependencies(ir);
        
        // Forward declarations
        code.push_str("// Forward declarations\r\n");
        for struct_def in &sorted_structs {
            if !config.should_include_type(&struct_def.name) {
                continue;
            }
            if should_skip_class(struct_def) || renders_as_type_alias(struct_def) {
                continue;
            }
            code.push_str(&format!("class {};\r\n", struct_def.name));
        }
        code.push_str("\r\n");
        
        // Class declarations
        code.push_str("// Wrapper class declarations\r\n\r\n");
        for struct_def in &sorted_structs {
            if !config.should_include_type(&struct_def.name) {
                continue;
            }
            self.generate_class_declaration(&mut code, struct_def, ir, config);
        }
        
        // Enum wrappers
        for enum_def in &ir.enums {
            if !config.should_include_type(&enum_def.name) {
                continue;
            }
            self.generate_enum_wrapper(&mut code, enum_def, config);
        }
        
        // Method implementations
        code.push_str("// Method implementations\r\n");
        code.push_str("// (Implemented after all classes are declared to avoid incomplete type errors)\r\n\r\n");
        
        for struct_def in &sorted_structs {
            if !config.should_include_type(&struct_def.name) {
                continue;
            }
            self.generate_method_implementations(&mut code, struct_def, ir, config);
        }
        
        // Close namespace
        code.push_str("} // namespace azul\r\n\r\n");
        
        // Include guards end
        code.push_str(&generate_include_guards_end(std));
        
        Ok(code)
    }
    
    fn generate_class_declaration(
        &self,
        code: &mut String,
        struct_def: &StructDef,
        ir: &CodegenIR,
        config: &CodegenConfig,
    ) {
        if should_skip_class(struct_def) {
            return;
        }
        
        let class_name = &struct_def.name;
        let c_type_name = config.apply_prefix(class_name);
        let is_copy_type = is_copy(struct_def);
        let needs_dtor = needs_destructor(struct_def);
        
        // Simple type alias for empty Copy types
        if renders_as_type_alias(struct_def) {
            code.push_str(&format!("using {} = {};\r\n\r\n", class_name, c_type_name));
            return;
        }
        
        // Class declaration
        code.push_str(&format!("class {} {{\r\n", class_name));
        
        code.push_str("private:\r\n");
        code.push_str(&format!("    {} inner_;\r\n\r\n", c_type_name));
        
        // Delete copy for non-Copy types
        if !is_copy_type {
            code.push_str(&format!("    {}(const {}&) = delete;\r\n", class_name, class_name));
            code.push_str(&format!("    {}& operator=(const {}&) = delete;\r\n\r\n", class_name, class_name));
        }
        
        code.push_str("public:\r\n");
        
        // Constructor from C type
        code.push_str(&format!(
            "    explicit {}({} inner) noexcept : inner_(inner) {{}}\r\n",
            class_name, c_type_name
        ));
        
        // Copy/move semantics
        self.generate_copy_move_semantics(code, class_name, &c_type_name, is_copy_type, needs_dtor);
        
        // Destructor
        self.generate_destructor(code, class_name, &c_type_name, needs_dtor);
        
        // Constructors with [[nodiscard]]
        self.generate_constructor_declarations(code, class_name, ir, config);
        
        // Instance methods
        self.generate_method_declarations(code, class_name, ir, config);
        
        // Accessor methods
        code.push_str("\r\n");
        code.push_str(&format!("    const {}& inner() const {{ return inner_; }}\r\n", c_type_name));
        code.push_str(&format!("    {}& inner() {{ return inner_; }}\r\n", c_type_name));
        code.push_str(&format!("    const {}* ptr() const {{ return &inner_; }}\r\n", c_type_name));
        code.push_str(&format!("    {}* ptr() {{ return &inner_; }}\r\n", c_type_name));
        code.push_str(&format!(
            "    {} release() {{ {} result = inner_; inner_ = {{}}; return result; }}\r\n",
            c_type_name, c_type_name
        ));
        
        // Type-specific methods
        if is_vec_type(struct_def) {
            self.generate_vec_methods(code, struct_def, config);
        }
        if is_string_type(struct_def) {
            self.generate_string_methods(code, struct_def, config);
        }
        if is_option_type(struct_def) {
            self.generate_option_methods(code, struct_def, config);
        }
        if is_result_type(struct_def) {
            self.generate_result_methods(code, struct_def, config);
        }
        
        code.push_str("};\r\n\r\n");
    }
    
    fn generate_method_implementations(
        &self,
        code: &mut String,
        struct_def: &StructDef,
        ir: &CodegenIR,
        config: &CodegenConfig,
    ) {
        if should_skip_class(struct_def) || renders_as_type_alias(struct_def) {
            return;
        }
        
        let class_name = &struct_def.name;
        
        // Constructor implementations
        for func in ir.functions.iter()
            .filter(|f| f.class_name == *class_name && is_constructor_or_default(f))
            .filter(|f| f.return_type.as_deref() == Some(class_name.as_str()))
        {
            let cpp_fn_name = escape_method_name(&func.method_name);
            let c_fn_name = &func.c_name;
            let cpp_args = generate_args_signature(&func.args, ir, config, false, class_name);
            let call_args = generate_call_args(&func.args, ir, false, class_name);
            
            code.push_str(&format!(
                "inline {} {}::{}({}) {{\r\n",
                class_name, class_name, cpp_fn_name, cpp_args
            ));
            code.push_str(&format!(
                "    return {}({}({}));\r\n",
                class_name, c_fn_name, call_args
            ));
            code.push_str("}\r\n\r\n");
        }
        
        // Factory methods
        for func in ir.functions.iter()
            .filter(|f| f.class_name == *class_name && is_constructor_or_default(f))
            .filter(|f| f.return_type.as_deref() != Some(class_name.as_str()))
        {
            let cpp_fn_name = escape_method_name(&func.method_name);
            let c_fn_name = &func.c_name;
            let cpp_return_type = get_cpp_return_type(func.return_type.as_deref(), ir);
            let cpp_args = generate_args_signature(&func.args, ir, config, false, class_name);
            let call_args = generate_call_args(&func.args, ir, false, class_name);
            
            code.push_str(&format!(
                "inline {} {}::{}({}) {{\r\n",
                cpp_return_type, class_name, cpp_fn_name, cpp_args
            ));
            code.push_str(&format!("    return {}({});\r\n", c_fn_name, call_args));
            code.push_str("}\r\n\r\n");
        }
        
        // Instance methods
        for func in ir.functions.iter()
            .filter(|f| f.class_name == *class_name)
            .filter(|f| !is_constructor_or_default(f))
            
        {
            let cpp_fn_name = escape_method_name(&func.method_name);
            let c_fn_name = &func.c_name;
            let has_self = func_has_self(func);
            let is_const = has_self && (matches!(func.kind, FunctionKind::Method) || func.is_const);
            let const_suffix = if is_const { " const" } else { "" };
            let cpp_return_type = get_cpp_return_type(func.return_type.as_deref(), ir);
            let cpp_args = generate_args_signature(&func.args, ir, config, true, class_name);
            let call_args = generate_call_args(&func.args, ir, true, class_name);
            
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
                if type_has_wrapper(return_type_str, ir) {
                    code.push_str(&format!(
                        "    return {}({}({}));\r\n",
                        return_type_str, c_fn_name, full_call_args
                    ));
                } else {
                    code.push_str(&format!("    return {}({});\r\n", c_fn_name, full_call_args));
                }
            }
            code.push_str("}\r\n\r\n");
        }
    }
    
    fn generate_destructor(
        &self,
        code: &mut String,
        class_name: &str,
        c_type_name: &str,
        needs_destructor: bool,
    ) {
        if needs_destructor {
            code.push_str(&format!(
                "    ~{}() {{ {}_delete(&inner_); }}\r\n",
                class_name, c_type_name
            ));
        } else {
            code.push_str(&format!("    ~{}() {{}}\r\n", class_name));
        }
    }
    
    fn generate_copy_move_semantics(
        &self,
        code: &mut String,
        class_name: &str,
        c_type_name: &str,
        is_copy: bool,
        needs_destructor: bool,
    ) {
        if is_copy {
            code.push_str(&format!(
                "    {}(const {}& other) noexcept : inner_(other.inner_) {{}}\r\n",
                class_name, class_name
            ));
            code.push_str(&format!(
                "    {}& operator=(const {}& other) noexcept {{ inner_ = other.inner_; return *this; }}\r\n",
                class_name, class_name
            ));
        } else {
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
    }
    
    fn generate_vec_methods(&self, code: &mut String, struct_def: &StructDef, _config: &CodegenConfig) {
        let elem_type = match get_vec_element_type(struct_def) {
            Some(t) => t,
            None => return,
        };
        
        let c_elem_type = if is_primitive(&elem_type) {
            primitive_to_c(&elem_type)
        } else {
            format!("Az{}", elem_type)
        };
        
        code.push_str("\r\n    // Iterator support\r\n");
        code.push_str(&format!("    const {}* begin() const {{ return inner_.ptr; }}\r\n", c_elem_type));
        code.push_str(&format!("    const {}* end() const {{ return inner_.ptr + inner_.len; }}\r\n", c_elem_type));
        code.push_str(&format!("    {}* begin() {{ return const_cast<{}*>(inner_.ptr); }}\r\n", c_elem_type, c_elem_type));
        code.push_str(&format!("    {}* end() {{ return const_cast<{}*>(inner_.ptr) + inner_.len; }}\r\n", c_elem_type, c_elem_type));
        code.push_str("    size_t size() const { return inner_.len; }\r\n");
        code.push_str("    bool empty() const { return inner_.len == 0; }\r\n");
        code.push_str(&format!("    const {}& operator[](size_t i) const {{ return inner_.ptr[i]; }}\r\n", c_elem_type));
        code.push_str(&format!("    {}& operator[](size_t i) {{ return const_cast<{}*>(inner_.ptr)[i]; }}\r\n", c_elem_type, c_elem_type));
        code.push_str(&format!(
            "    std::vector<{}> toStdVector() const {{ return std::vector<{}>(begin(), end()); }}\r\n",
            c_elem_type, c_elem_type
        ));
    }
    
    fn generate_string_methods(&self, code: &mut String, _struct_def: &StructDef, _config: &CodegenConfig) {
        code.push_str("\r\n    // String methods\r\n");
        code.push_str("    String(const char* s) : inner_(AzString_copyFromBytes(reinterpret_cast<const uint8_t*>(s), 0, std::strlen(s))) {}\r\n");
        code.push_str("    String(const std::string& s) : inner_(AzString_copyFromBytes(reinterpret_cast<const uint8_t*>(s.c_str()), 0, s.size())) {}\r\n");
        // C++17: std::string_view
        code.push_str("    String(std::string_view sv) : inner_(AzString_copyFromBytes(reinterpret_cast<const uint8_t*>(sv.data()), 0, sv.size())) {}\r\n");
        code.push_str("    const char* c_str() const { return reinterpret_cast<const char*>(inner_.vec.ptr); }\r\n");
        code.push_str("    size_t length() const { return inner_.vec.len; }\r\n");
        code.push_str("    std::string toStdString() const { return std::string(c_str(), length()); }\r\n");
        code.push_str("    operator std::string() const { return toStdString(); }\r\n");
        // C++17: std::string_view
        code.push_str("    std::string_view toStringView() const { return std::string_view(c_str(), length()); }\r\n");
        code.push_str("    operator std::string_view() const { return toStringView(); }\r\n");
    }
    
    fn generate_option_methods(&self, code: &mut String, struct_def: &StructDef, config: &CodegenConfig) {
        let c_type_name = config.apply_prefix(&struct_def.name);
        let inner_type = get_option_inner_type(struct_def).unwrap_or_else(|| "void".to_string());
        let c_inner_type = if is_primitive(&inner_type) {
            primitive_to_c(&inner_type)
        } else {
            format!("Az{}", inner_type)
        };
        
        code.push_str("\r\n    // Option methods\r\n");
        code.push_str(&format!("    bool isSome() const {{ return inner_.Some.tag == {}_Tag_Some; }}\r\n", c_type_name));
        code.push_str(&format!("    bool isNone() const {{ return inner_.Some.tag == {}_Tag_None; }}\r\n", c_type_name));
        code.push_str("    explicit operator bool() const { return isSome(); }\r\n");
        code.push_str(&format!("    const {}& unwrap() const {{ return inner_.Some.payload; }}\r\n", c_inner_type));
        code.push_str(&format!("    {}& unwrap() {{ return inner_.Some.payload; }}\r\n", c_inner_type));
        code.push_str(&format!(
            "    {} unwrapOr(const {}& def) const {{ return isSome() ? inner_.Some.payload : def; }}\r\n",
            c_inner_type, c_inner_type
        ));
        // C++17: std::optional
        code.push_str(&format!(
            "    std::optional<{}> toStdOptional() const {{ return isSome() ? std::optional<{}>(inner_.Some.payload) : std::nullopt; }}\r\n",
            c_inner_type, c_inner_type
        ));
        code.push_str(&format!(
            "    operator std::optional<{}>() const {{ return toStdOptional(); }}\r\n",
            c_inner_type
        ));
    }
    
    fn generate_result_methods(&self, code: &mut String, struct_def: &StructDef, config: &CodegenConfig) {
        let c_type_name = config.apply_prefix(&struct_def.name);
        
        code.push_str("\r\n    // Result methods\r\n");
        code.push_str(&format!("    bool isOk() const {{ return inner_.Ok.tag == {}_Tag_Ok; }}\r\n", c_type_name));
        code.push_str(&format!("    bool isErr() const {{ return inner_.Ok.tag == {}_Tag_Err; }}\r\n", c_type_name));
        code.push_str("    explicit operator bool() const { return isOk(); }\r\n");
    }
}

// ============================================================================
// Helper methods
// ============================================================================

impl Cpp17Generator {
    fn sort_types_by_dependencies<'a>(&self, ir: &'a CodegenIR) -> Vec<&'a StructDef> {
        let mut structs: Vec<&StructDef> = ir.structs.iter().collect();
        structs.sort_by_key(|s| s.sort_order);
        structs
    }
    
    fn generate_constructor_declarations(
        &self,
        code: &mut String,
        class_name: &str,
        ir: &CodegenIR,
        config: &CodegenConfig,
    ) {
        // True constructors with [[nodiscard]]
        let constructors: Vec<_> = ir.functions.iter()
            .filter(|f| f.class_name == class_name && is_constructor_or_default(f))
            .filter(|f| f.return_type.as_deref() == Some(class_name))
            .collect();
        
        if !constructors.is_empty() {
            code.push_str("\r\n");
            for func in constructors {
                let cpp_fn_name = escape_method_name(&func.method_name);
                let cpp_args = generate_args_signature(&func.args, ir, config, false, class_name);
                code.push_str(&format!(
                    "    [[nodiscard]] static {} {}({});\r\n",
                    class_name, cpp_fn_name, cpp_args
                ));
            }
        }
        
        // Factory methods with [[nodiscard]]
        let factories: Vec<_> = ir.functions.iter()
            .filter(|f| f.class_name == class_name && is_constructor_or_default(f))
            .filter(|f| f.return_type.as_deref() != Some(class_name))
            .collect();
        
        for func in factories {
            let cpp_fn_name = escape_method_name(&func.method_name);
            let cpp_args = generate_args_signature(&func.args, ir, config, false, class_name);
            let cpp_return_type = get_cpp_return_type(func.return_type.as_deref(), ir);
            code.push_str(&format!(
                "    [[nodiscard]] static {} {}({});\r\n",
                cpp_return_type, cpp_fn_name, cpp_args
            ));
        }
    }
    
    fn generate_method_declarations(
        &self,
        code: &mut String,
        class_name: &str,
        ir: &CodegenIR,
        config: &CodegenConfig,
    ) {
        let methods: Vec<_> = ir.functions.iter()
            .filter(|f| f.class_name == class_name)
            .filter(|f| !is_constructor_or_default(f))
            
            .collect();
        
        if !methods.is_empty() {
            code.push_str("\r\n");
            for func in methods {
                let cpp_fn_name = escape_method_name(&func.method_name);
                let has_self = func_has_self(func);
                let is_const = has_self && (matches!(func.kind, FunctionKind::Method) || func.is_const);
                let const_suffix = if is_const { " const" } else { "" };
                let static_prefix = if !has_self { "static " } else { "" };
                let cpp_return_type = get_cpp_return_type(func.return_type.as_deref(), ir);
                let cpp_args = generate_args_signature(&func.args, ir, config, true, class_name);
                code.push_str(&format!(
                    "    {}{} {}({}){};\r\n",
                    static_prefix, cpp_return_type, cpp_fn_name, cpp_args, const_suffix
                ));
            }
        }
    }
    
    fn generate_enum_wrapper(&self, code: &mut String, enum_def: &EnumDef, config: &CodegenConfig) {
        if !enum_def.generic_params.is_empty() {
            return;
        }
        
        let enum_name = &enum_def.name;
        let c_type_name = config.apply_prefix(enum_name);
        
        if enum_def.is_union {
            code.push_str(&format!("// {} is a tagged union - use C API\r\n", enum_name));
        }
        code.push_str(&format!("using {} = {};\r\n\r\n", enum_name, c_type_name));
    }
}
