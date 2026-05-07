//! C++20 and C++23 Generators
//!
//! These generators produce C++20/C++23-compatible code with modern features:
//!
//! C++20:
//! - std::span support for Vec types (zero-copy view)
//! - Concepts (future)
//! - Ranges (future)
//!
//! C++23:
//! - std::expected support for Result types

use super::super::config::*;
use super::super::ir::*;
use super::{common::*, CppDialect};
use anyhow::Result;

/// C++20 dialect generator
pub struct Cpp20Generator;

/// C++23 dialect generator (extends C++20)
pub struct Cpp23Generator;

// ============================================================================
// C++20 Generator
// ============================================================================

impl CppDialect for Cpp20Generator {
    fn standard(&self) -> CppStandard {
        CppStandard::Cpp20
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

        // AZ_REFLECT macro - C++11+ uses template-reflection helpers instead.
        if !std.has_move_semantics() {
            code.push_str(&generate_reflect_macro(std));
        } else {
            code.push_str(&generate_az_string_from_literal_helper(std));
        }

        // Open namespace
        code.push_str("namespace azul {\r\n\r\n");

        // Synthesize struct entries for Option/Result tagged-union enums.
        let synthesized = synthesize_option_result_structs(ir);
        let sorted_structs = self.sort_types_by_dependencies(ir);
        let all_structs: Vec<&StructDef> =
            sorted_structs.iter().copied().chain(synthesized.iter()).collect();

        // Forward declarations
        code.push_str("// Forward declarations\r\n");
        for struct_def in &all_structs {
            if !config.should_include_type(&struct_def.name) {
                continue;
            }
            if should_skip_class(struct_def) || renders_as_type_alias(struct_def) {
                continue;
            }
            code.push_str(&format!("class {};\r\n", struct_def.name));
        }
        code.push_str("\r\n");

        // Template-reflection scaffolding (detail::type_id_holder /
        // detail::type_destructor + ReflectableModel concept) before class
        // declarations so RefAny's template members can resolve the names.
        code.push_str(&generate_template_reflection(std));

        // Free-function downcast helpers on AzRefAny.
        code.push_str(&generate_refany_freefn_downcasts(std));

        // Class declarations
        code.push_str("// Wrapper class declarations\r\n\r\n");
        for struct_def in &all_structs {
            if !config.should_include_type(&struct_def.name) {
                continue;
            }
            self.generate_class_declaration(&mut code, struct_def, ir, config);
        }

        // Enum wrappers (skip Option/Result — those got real classes above).
        for enum_def in &ir.enums {
            if !config.should_include_type(&enum_def.name) {
                continue;
            }
            if matches!(enum_def.category, TypeCategory::Option | TypeCategory::Result) {
                continue;
            }
            self.generate_enum_wrapper(&mut code, enum_def, config);
        }

        // Method implementations
        code.push_str("// Method implementations\r\n");
        code.push_str("// (Implemented after all classes are declared to avoid incomplete type errors)\r\n\r\n");

        for struct_def in &all_structs {
            if !config.should_include_type(&struct_def.name) {
                continue;
            }
            self.generate_method_implementations(&mut code, struct_def, ir, config);
        }

        // Close namespace
        code.push_str("} // namespace azul\r\n\r\n");

        // Structured-binding specializations (namespace std).
        code.push_str(&generate_structured_binding_specs(ir));

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
        emit_class_declaration_cpp20_or_later(self, code, struct_def, ir, config);
    }

    fn generate_method_implementations(
        &self,
        code: &mut String,
        struct_def: &StructDef,
        ir: &CodegenIR,
        config: &CodegenConfig,
    ) {
        // Delegate to shared implementation
        generate_method_implementations_shared(self, code, struct_def, ir, config);
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

    fn generate_vec_methods(
        &self,
        code: &mut String,
        struct_def: &StructDef,
        _config: &CodegenConfig,
    ) {
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
        code.push_str(&format!(
            "    std::vector<{}> toStdVector() const {{ return std::vector<{}>(begin(), end()); }}\r\n",
            c_elem_type, c_elem_type
        ));
        // C++20: std::span
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

    fn generate_string_methods(
        &self,
        code: &mut String,
        _struct_def: &StructDef,
        _config: &CodegenConfig,
    ) {
        code.push_str("\r\n    // String methods\r\n");
        code.push_str("    String(const char* s) : inner_(AzString_copyFromBytes(reinterpret_cast<const uint8_t*>(s), 0, std::strlen(s))) {}\r\n");
        code.push_str("    String(const std::string& s) : inner_(AzString_copyFromBytes(reinterpret_cast<const uint8_t*>(s.c_str()), 0, s.size())) {}\r\n");
        code.push_str("    String(std::string_view sv) : inner_(AzString_copyFromBytes(reinterpret_cast<const uint8_t*>(sv.data()), 0, sv.size())) {}\r\n");
        code.push_str("    const char* c_str() const { return reinterpret_cast<const char*>(inner_.vec.ptr); }\r\n");
        code.push_str("    size_t length() const { return inner_.vec.len; }\r\n");
        code.push_str(
            "    std::string toStdString() const { return std::string(c_str(), length()); }\r\n",
        );
        code.push_str("    operator std::string() const { return toStdString(); }\r\n");
        code.push_str("    std::string_view toStringView() const { return std::string_view(c_str(), length()); }\r\n");
        code.push_str("    operator std::string_view() const { return toStringView(); }\r\n");
    }

    fn generate_option_methods(
        &self,
        code: &mut String,
        struct_def: &StructDef,
        ir: &CodegenIR,
        config: &CodegenConfig,
    ) {
        let c_type_name = config.apply_prefix(&struct_def.name);
        let inner_type =
            get_option_inner_type_ir(struct_def, ir).unwrap_or_else(|| "void".to_string());
        let c_inner_type = if is_primitive(&inner_type) {
            primitive_to_c(&inner_type)
        } else {
            format!("Az{}", inner_type)
        };

        code.push_str("\r\n    // Option methods\r\n");
        code.push_str(&format!(
            "    bool isSome() const {{ return inner_.Some.tag == {}_Tag_Some; }}\r\n",
            c_type_name
        ));
        code.push_str(&format!(
            "    bool isNone() const {{ return inner_.Some.tag == {}_Tag_None; }}\r\n",
            c_type_name
        ));
        code.push_str("    explicit operator bool() const { return isSome(); }\r\n");
        code.push_str(&format!(
            "    const {}& unwrap() const {{ return inner_.Some.payload; }}\r\n",
            c_inner_type
        ));
        code.push_str(&format!(
            "    {}& unwrap() {{ return inner_.Some.payload; }}\r\n",
            c_inner_type
        ));
        code.push_str(&format!(
            "    {} unwrapOr(const {}& def) const {{ return isSome() ? inner_.Some.payload : def; }}\r\n",
            c_inner_type, c_inner_type
        ));
        code.push_str(&format!(
            "    std::optional<{}> toStdOptional() const {{ return isSome() ? std::optional<{}>(inner_.Some.payload) : std::nullopt; }}\r\n",
            c_inner_type, c_inner_type
        ));
        code.push_str(&format!(
            "    operator std::optional<{}>() const {{ return toStdOptional(); }}\r\n",
            c_inner_type
        ));
    }

    fn generate_result_methods(
        &self,
        code: &mut String,
        struct_def: &StructDef,
        _ir: &CodegenIR,
        config: &CodegenConfig,
    ) {
        let c_type_name = config.apply_prefix(&struct_def.name);

        code.push_str("\r\n    // Result methods\r\n");
        code.push_str(&format!(
            "    bool isOk() const {{ return inner_.Ok.tag == {}_Tag_Ok; }}\r\n",
            c_type_name
        ));
        code.push_str(&format!(
            "    bool isErr() const {{ return inner_.Ok.tag == {}_Tag_Err; }}\r\n",
            c_type_name
        ));
        code.push_str("    explicit operator bool() const { return isOk(); }\r\n");
    }
}

// ============================================================================
// C++23 Generator (extends C++20 with std::expected)
// ============================================================================

impl CppDialect for Cpp23Generator {
    fn standard(&self) -> CppStandard {
        CppStandard::Cpp23
    }

    fn generate(&self, ir: &CodegenIR, config: &CodegenConfig) -> Result<String> {
        // Same as C++20 but with different standard
        let mut code = String::new();
        let std = self.standard();

        code.push_str(&generate_header_comment(std));
        code.push_str(&generate_feature_docs(std));
        code.push_str("// =============================================================================\r\n\r\n");
        code.push_str(&generate_include_guards_begin(std));
        code.push_str(&generate_includes(std));
        // C++11+ uses template-reflection helpers instead of AZ_REFLECT.
        if !std.has_move_semantics() {
            code.push_str(&generate_reflect_macro(std));
        } else {
            code.push_str(&generate_az_string_from_literal_helper(std));
        }

        code.push_str("namespace azul {\r\n\r\n");

        let synthesized = synthesize_option_result_structs(ir);
        let sorted_structs = self.sort_types_by_dependencies(ir);
        let all_structs: Vec<&StructDef> =
            sorted_structs.iter().copied().chain(synthesized.iter()).collect();

        // Forward declarations
        code.push_str("// Forward declarations\r\n");
        for struct_def in &all_structs {
            if !config.should_include_type(&struct_def.name) {
                continue;
            }
            if should_skip_class(struct_def) || renders_as_type_alias(struct_def) {
                continue;
            }
            code.push_str(&format!("class {};\r\n", struct_def.name));
        }
        code.push_str("\r\n");

        // Template-reflection scaffolding before class declarations.
        code.push_str(&generate_template_reflection(std));

        // Class declarations
        code.push_str("// Wrapper class declarations\r\n\r\n");
        for struct_def in &all_structs {
            if !config.should_include_type(&struct_def.name) {
                continue;
            }
            self.generate_class_declaration(&mut code, struct_def, ir, config);
        }

        for enum_def in &ir.enums {
            if !config.should_include_type(&enum_def.name) {
                continue;
            }
            if matches!(enum_def.category, TypeCategory::Option | TypeCategory::Result) {
                continue;
            }
            self.generate_enum_wrapper(&mut code, enum_def, config);
        }

        code.push_str("// Method implementations\r\n");
        code.push_str("// (Implemented after all classes are declared to avoid incomplete type errors)\r\n\r\n");

        for struct_def in &all_structs {
            if !config.should_include_type(&struct_def.name) {
                continue;
            }
            self.generate_method_implementations(&mut code, struct_def, ir, config);
        }

        code.push_str("} // namespace azul\r\n\r\n");

        // Structured-binding specializations (namespace std).
        code.push_str(&generate_structured_binding_specs(ir));

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
        emit_class_declaration_cpp20_or_later(self, code, struct_def, ir, config);
    }

    fn generate_method_implementations(
        &self,
        code: &mut String,
        struct_def: &StructDef,
        ir: &CodegenIR,
        config: &CodegenConfig,
    ) {
        generate_method_implementations_shared(self, code, struct_def, ir, config);
    }

    fn generate_destructor(
        &self,
        code: &mut String,
        class_name: &str,
        c_type_name: &str,
        needs_destructor: bool,
    ) {
        Cpp20Generator.generate_destructor(code, class_name, c_type_name, needs_destructor);
    }

    fn generate_copy_move_semantics(
        &self,
        code: &mut String,
        class_name: &str,
        c_type_name: &str,
        is_copy: bool,
        needs_destructor: bool,
    ) {
        Cpp20Generator.generate_copy_move_semantics(
            code,
            class_name,
            c_type_name,
            is_copy,
            needs_destructor,
        );
    }

    fn generate_vec_methods(
        &self,
        code: &mut String,
        struct_def: &StructDef,
        config: &CodegenConfig,
    ) {
        Cpp20Generator.generate_vec_methods(code, struct_def, config);
    }

    fn generate_string_methods(
        &self,
        code: &mut String,
        struct_def: &StructDef,
        config: &CodegenConfig,
    ) {
        Cpp20Generator.generate_string_methods(code, struct_def, config);
    }

    fn generate_option_methods(
        &self,
        code: &mut String,
        struct_def: &StructDef,
        ir: &CodegenIR,
        config: &CodegenConfig,
    ) {
        Cpp20Generator.generate_option_methods(code, struct_def, ir, config);
    }

    fn generate_result_methods(
        &self,
        code: &mut String,
        struct_def: &StructDef,
        ir: &CodegenIR,
        config: &CodegenConfig,
    ) {
        Cpp20Generator.generate_result_methods(code, struct_def, ir, config);
    }
}

/// Emit a wrapper class declaration for a struct on the C++20-or-later path.
/// The C++23 generator uses this same body but its `standard()` returns
/// `Cpp23`, so the conditional Result extras kick in automatically.
fn emit_class_declaration_cpp20_or_later(
    gen: &(impl CppDialect + ?Sized),
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

    if renders_as_type_alias(struct_def) {
        code.push_str(&format!("using {} = {};\r\n\r\n", class_name, c_type_name));
        return;
    }

    code.push_str(&format!("class {} {{\r\n", class_name));
    code.push_str("private:\r\n");
    code.push_str(&format!("    {} inner_;\r\n\r\n", c_type_name));

    if !is_copy_type {
        code.push_str(&format!(
            "    {}(const {}&) = delete;\r\n",
            class_name, class_name
        ));
        code.push_str(&format!(
            "    {}& operator=(const {}&) = delete;\r\n\r\n",
            class_name, class_name
        ));
    }

    code.push_str("public:\r\n");
    let ctor_explicit = if is_copy_type { "" } else { "explicit " };
    code.push_str(&format!(
        "    {}{}({} inner) noexcept : inner_(inner) {{}}\r\n",
        ctor_explicit, class_name, c_type_name
    ));

    gen.generate_copy_move_semantics(code, class_name, &c_type_name, is_copy_type, needs_dtor);
    gen.generate_destructor(code, class_name, &c_type_name, needs_dtor);

    // Constructor & method declarations are not on the trait, so we have to
    // delegate to a generator that has them. Cpp20Generator owns these helpers
    // and they don't depend on the standard, so we always call its versions.
    Cpp20Generator.generate_constructor_declarations(code, class_name, ir, config);
    emit_method_declarations(gen.standard(), code, class_name, ir, config);

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
    code.push_str(&format!(
        "    {} release() {{ {} result = inner_; inner_ = {{}}; return result; }}\r\n",
        c_type_name, c_type_name
    ));
    // Implicit r-value conversion: enables `return Wrapper::create();` from a
    // C-ABI callback without explicit `.release()`.
    code.push_str(&format!(
        "    operator {}() && noexcept {{ {} result = inner_; inner_ = {{}}; return result; }}\r\n",
        c_type_name, c_type_name
    ));

    if is_vec_type(struct_def) {
        gen.generate_vec_methods(code, struct_def, config);
    }
    if is_string_type(struct_def) {
        gen.generate_string_methods(code, struct_def, config);
    }
    if is_option_type(struct_def) {
        gen.generate_option_methods(code, struct_def, ir, config);
    }
    if is_result_type(struct_def) {
        gen.generate_result_methods(code, struct_def, ir, config);
        if gen.standard() >= CppStandard::Cpp23 {
            emit_cpp23_result_extras(code, struct_def, ir, config);
        }
    }
    if matches!(struct_def.category, TypeCategory::RefAny) {
        code.push_str(&generate_refany_template_members(gen.standard()));
    }

    code.push_str("};\r\n\r\n");
}

/// Emit C++23-specific Result extras: `toStdExpected() &&` and an implicit
/// `operator std::expected<Ok, Err>()`. The Ok/Err payload types are looked
/// up from the sibling enum's `EnumVariantKind::Tuple` shape — generic for
/// every `ResultXxx` enum.
fn emit_cpp23_result_extras(
    code: &mut String,
    struct_def: &StructDef,
    ir: &CodegenIR,
    config: &CodegenConfig,
) {
    let enum_def = match ir.find_enum(&struct_def.name) {
        Some(e) => e,
        None => return,
    };
    let (ok_t, err_t) = match get_result_payload_types(enum_def) {
        Some(p) => p,
        None => return,
    };

    let c_ok = if is_primitive(&ok_t) {
        primitive_to_c(&ok_t)
    } else {
        format!("Az{}", ok_t)
    };
    let c_err = if is_primitive(&err_t) {
        primitive_to_c(&err_t)
    } else {
        format!("Az{}", err_t)
    };
    let c_type_name = config.apply_prefix(&struct_def.name);

    code.push_str(&format!(
        "    std::expected<{ok}, {err}> toStdExpected() && {{\r\n        if (isOk()) {{\r\n            {ok} v = inner_.Ok.payload;\r\n            inner_ = {{}};\r\n            return std::expected<{ok}, {err}>(std::move(v));\r\n        }} else {{\r\n            {err} e = inner_.Err.payload;\r\n            inner_ = {{}};\r\n            return std::expected<{ok}, {err}>(std::unexpected<{err}>(std::move(e)));\r\n        }}\r\n    }}\r\n",
        ok = c_ok,
        err = c_err,
    ));
    code.push_str(&format!(
        "    operator std::expected<{ok}, {err}>() && {{ return std::move(*this).toStdExpected(); }}\r\n",
        ok = c_ok,
        err = c_err,
    ));
    let _ = c_type_name;
}

// ============================================================================
// Shared helper implementations
// ============================================================================

impl Cpp20Generator {
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
        let constructors: Vec<_> = ir
            .functions
            .iter()
            .filter(|f| f.class_name == class_name && is_constructor_or_default(f))
            .filter(|f| f.return_type.as_deref() == Some(class_name))
            .collect();

        if !constructors.is_empty() {
            code.push_str("\r\n");
            for func in constructors {
                let cpp_fn_name = escape_method_name(&func.method_name);
                let substitute = should_substitute_callbacks(func);
                let cpp_args = generate_args_signature_ex(
                    &func.args, ir, config, false, class_name, substitute,
                );
                code.push_str(&format!(
                    "    [[nodiscard]] static {} {}({});\r\n",
                    class_name, cpp_fn_name, cpp_args
                ));
            }
        }

        let factories: Vec<_> = ir
            .functions
            .iter()
            .filter(|f| f.class_name == class_name && is_constructor_or_default(f))
            .filter(|f| f.return_type.as_deref() != Some(class_name))
            .collect();

        for func in factories {
            let cpp_fn_name = escape_method_name(&func.method_name);
            let substitute = should_substitute_callbacks(func);
            let cpp_args =
                generate_args_signature_ex(&func.args, ir, config, false, class_name, substitute);
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
        emit_method_declarations(CppStandard::Cpp20, code, class_name, ir, config);
    }

    fn generate_enum_wrapper(&self, code: &mut String, enum_def: &EnumDef, config: &CodegenConfig) {
        if !enum_def.generic_params.is_empty() {
            return;
        }

        let enum_name = &enum_def.name;
        let c_type_name = config.apply_prefix(enum_name);

        if enum_def.is_union {
            code.push_str(&format!(
                "// {} is a tagged union - use C API\r\n",
                enum_name
            ));
        }
        code.push_str(&format!("using {} = {};\r\n\r\n", enum_name, c_type_name));
    }
}

impl Cpp23Generator {
    fn sort_types_by_dependencies<'a>(&self, ir: &'a CodegenIR) -> Vec<&'a StructDef> {
        let mut structs: Vec<&StructDef> = ir.structs.iter().collect();
        structs.sort_by_key(|s| s.sort_order);
        structs
    }

    fn generate_enum_wrapper(&self, code: &mut String, enum_def: &EnumDef, config: &CodegenConfig) {
        Cpp20Generator.generate_enum_wrapper(code, enum_def, config);
    }
}

/// Emit instance-method declarations into the class body. Standard-aware so
/// the C++23 path can swap the `&`-qualified builder form for a deducing-`this`
/// `template<class Self> auto with_xxx(this Self&& self, …)` declaration.
fn emit_method_declarations(
    standard: CppStandard,
    code: &mut String,
    class_name: &str,
    ir: &CodegenIR,
    config: &CodegenConfig,
) {
    let methods: Vec<_> = ir
        .functions
        .iter()
        .filter(|f| f.class_name == class_name)
        .filter(|f| !is_constructor_or_default(f))
        .collect();

    if methods.is_empty() {
        return;
    }

    code.push_str("\r\n");
    for func in methods {
        let cpp_fn_name = escape_method_name(&func.method_name);
        let has_self = func_has_self(func);
        let cpp_return_type = get_cpp_return_type(func.return_type.as_deref(), ir);
        let substitute = should_substitute_callbacks(func);

        // C++23 deducing-`this` form for builder methods (`with_*` /
        // `_with_*`). The declaration goes here; the matching template
        // definition is emitted out-of-class in
        // `generate_method_implementations_shared` (parameter types may be
        // forward-declared at this point, so the body can't be parsed yet).
        if standard >= CppStandard::Cpp23 && is_builder_method(func) {
            let cpp_args = generate_args_signature_ex(
                &func.args, ir, config, true, class_name, substitute,
            );
            let comma = if cpp_args.is_empty() { "" } else { ", " };
            code.push_str(&format!(
                "    template<class Self> {} {}(this Self&& self{}{});\r\n",
                cpp_return_type, cpp_fn_name, comma, cpp_args
            ));
            continue;
        }

        let is_const =
            has_self && (matches!(func.kind, FunctionKind::Method) || func.is_const);
        let const_suffix = if is_const { " const" } else { "" };
        let static_prefix = if !has_self { "static " } else { "" };
        let cpp_args =
            generate_args_signature_ex(&func.args, ir, config, true, class_name, substitute);
        code.push_str(&format!(
            "    {}{} {}({}){};\r\n",
            static_prefix, cpp_return_type, cpp_fn_name, cpp_args, const_suffix
        ));
    }
}

/// Shared method implementation generator
fn generate_method_implementations_shared(
    dialect: &dyn CppDialect,
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
    for func in ir
        .functions
        .iter()
        .filter(|f| f.class_name == *class_name && is_constructor_or_default(f))
        .filter(|f| f.return_type.as_deref() == Some(class_name.as_str()))
    {
        let cpp_fn_name = escape_method_name(&func.method_name);
        let c_fn_name = &func.c_name;
        let substitute = should_substitute_callbacks(func);
        let cpp_args =
            generate_args_signature_ex(&func.args, ir, config, false, class_name, substitute);
        let call_args = generate_call_args_ex(&func.args, ir, false, class_name, substitute);

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
    for func in ir
        .functions
        .iter()
        .filter(|f| f.class_name == *class_name && is_constructor_or_default(f))
        .filter(|f| f.return_type.as_deref() != Some(class_name.as_str()))
    {
        let cpp_fn_name = escape_method_name(&func.method_name);
        let c_fn_name = &func.c_name;
        let cpp_return_type = get_cpp_return_type(func.return_type.as_deref(), ir);
        let substitute = should_substitute_callbacks(func);
        let cpp_args =
            generate_args_signature_ex(&func.args, ir, config, false, class_name, substitute);
        let call_args = generate_call_args_ex(&func.args, ir, false, class_name, substitute);

        code.push_str(&format!(
            "inline {} {}::{}({}) {{\r\n",
            cpp_return_type, class_name, cpp_fn_name, cpp_args
        ));
        let return_type_str = func.return_type.as_deref().unwrap_or("");
        if type_has_wrapper(return_type_str, ir) {
            code.push_str(&format!(
                "    return {}({}({}));\r\n",
                return_type_str, c_fn_name, call_args
            ));
        } else {
            code.push_str(&format!("    return {}({});\r\n", c_fn_name, call_args));
        }
        code.push_str("}\r\n\r\n");
    }

    // Instance methods
    for func in ir
        .functions
        .iter()
        .filter(|f| f.class_name == *class_name)
        .filter(|f| !is_constructor_or_default(f))
    {
        // C++23 deducing-`this` builder methods get an out-of-class template
        // definition matching the `template<class Self>` declaration emitted
        // by `emit_method_declarations`.
        if dialect.standard() >= CppStandard::Cpp23 && is_builder_method(func) {
            let cpp_fn_name = escape_method_name(&func.method_name);
            let cpp_return_type = get_cpp_return_type(func.return_type.as_deref(), ir);
            let substitute = should_substitute_callbacks(func);
            let cpp_args = generate_args_signature_ex(
                &func.args, ir, config, true, class_name, substitute,
            );
            let call_args =
                generate_call_args_ex(&func.args, ir, true, class_name, substitute);
            let self_is_value = func
                .args
                .first()
                .map(|a| a.ref_kind == ArgRefKind::Owned)
                .unwrap_or(false);
            // The C signature dictates whether the wrapper passes self by
            // value or by pointer. Mirror what the non-deducing-this path
            // would produce.
            let self_arg = if self_is_value {
                "self.inner_"
            } else {
                "&self.inner_"
            };
            let comma = if cpp_args.is_empty() { "" } else { ", " };
            let full_call_args = if call_args.is_empty() {
                self_arg.to_string()
            } else {
                format!("{}, {}", self_arg, call_args)
            };
            let return_type_str = func.return_type.as_deref().unwrap_or("");
            code.push_str(&format!(
                "template<class Self>\r\ninline {} {}::{}(this Self&& self{}{}) {{\r\n",
                cpp_return_type, class_name, cpp_fn_name, comma, cpp_args
            ));
            if cpp_return_type == "void" {
                code.push_str(&format!("    {}({});\r\n", func.c_name, full_call_args));
            } else if type_has_wrapper(return_type_str, ir) {
                code.push_str(&format!(
                    "    return {}({}({}));\r\n",
                    return_type_str, func.c_name, full_call_args
                ));
            } else {
                code.push_str(&format!("    return {}({});\r\n", func.c_name, full_call_args));
            }
            code.push_str("}\r\n\r\n");
            continue;
        }

        let cpp_fn_name = escape_method_name(&func.method_name);
        let c_fn_name = &func.c_name;
        let has_self = func_has_self(func);
        let is_const = has_self && (matches!(func.kind, FunctionKind::Method) || func.is_const);
        let const_suffix = if is_const { " const" } else { "" };
        let cpp_return_type = get_cpp_return_type(func.return_type.as_deref(), ir);
        let substitute = should_substitute_callbacks(func);
        let cpp_args =
            generate_args_signature_ex(&func.args, ir, config, true, class_name, substitute);
        let call_args = generate_call_args_ex(&func.args, ir, true, class_name, substitute);

        let full_call_args = if has_self {
            let self_is_value = func
                .args
                .first()
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
                code.push_str(&format!(
                    "    return {}({});\r\n",
                    c_fn_name, full_call_args
                ));
            }
        }
        code.push_str("}\r\n\r\n");
    }
}
