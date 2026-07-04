//! C++14 Generator
//!
//! C++14 reuses the C++11 emission path but reports its own standard so that
//! the standard-aware helpers (template reflection, includes) emit C++14
//! features — most notably `azul::type_id_v<T>`.

use super::super::config::*;
use super::super::ir::*;
use super::cpp11::emit_class_declaration_cpp11_or_later;
use super::{common::*, CppDialect, Cpp11Generator};
use anyhow::Result;

pub struct Cpp14Generator;

impl CppDialect for Cpp14Generator {
    fn standard(&self) -> CppStandard {
        CppStandard::Cpp14
    }

    fn generate(&self, ir: &CodegenIR, config: &CodegenConfig) -> Result<String> {
        let mut code = String::new();
        let std = self.standard();

        code.push_str(&generate_header_comment(std));
        code.push_str("// =============================================================================\r\n\r\n");
        code.push_str(&generate_include_guards_begin(std));
        code.push_str(&generate_includes(std));
        if !std.has_move_semantics() {
            code.push_str(&generate_reflect_macro(std));
        } else {
            code.push_str(&generate_az_string_from_literal_helper(std));
        }
        code.push_str("namespace azul {\r\n\r\n");

        let synthesized = synthesize_option_result_structs(ir);
        let mut sorted_structs: Vec<&StructDef> = ir.structs.iter().collect();
        sorted_structs.sort_by_key(|s| s.sort_order);
        let all_structs: Vec<&StructDef> =
            sorted_structs.iter().copied().chain(synthesized.iter()).collect();

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

        // Non-prefixed aliases for the raw C callback fn-ptr typedefs. Must
        // precede the class declarations, whose method signatures use them.
        code.push_str(&generate_callback_typedef_aliases(ir, config, false));

        // Template-reflection scaffolding before class declarations so
        // RefAny::create<T> can resolve detail::type_id_holder at parse time.
        code.push_str(&generate_template_reflection(std));

        // Free-function downcast helpers on AzRefAny.
        code.push_str(&generate_refany_freefn_downcasts(std));

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
            if !enum_def.generic_params.is_empty() {
                continue;
            }
            if matches!(
                enum_def.category,
                TypeCategory::Option | TypeCategory::Result | TypeCategory::DestructorOrClone
            ) {
                continue;
            }
            // Cpp11Generator owns the enum-wrapper helper, but it's not on the
            // trait. Inline the simple alias/constants path here (must match
            // Cpp11Generator::generate_enum_wrapper).
            let c_type_name = config.apply_prefix(&enum_def.name);
            if enum_def.is_union {
                code.push_str(&format!("// {} is a tagged union - use C API\r\n", enum_def.name));
                code.push_str(&format!("using {} = {};\r\n\r\n", enum_def.name, c_type_name));
            } else {
                // Unit enum: scoped, non-prefixed value constants
                // (`Update::RefreshDom`). C++14 namespace-scope `constexpr`
                // has internal linkage → template-static ODR-safe form.
                code.push_str(&generate_enum_constants_extern(enum_def, config, false));
            }
        }

        code.push_str("// Method implementations\r\n");
        code.push_str("// (Implemented after all classes are declared to avoid incomplete type errors)\r\n\r\n");

        for struct_def in &all_structs {
            if !config.should_include_type(&struct_def.name) {
                continue;
            }
            self.generate_method_implementations(&mut code, struct_def, ir, config);
        }

        // Out-of-class definition for RefAny::type_id_v (C++14 only).
        code.push_str(&generate_refany_type_id_v_definition(std));

        code.push_str("} // namespace azul\r\n\r\n");
        // Structured bindings on Result types use std::optional + if-constexpr,
        // which are C++17 features.
        if std.has_optional() {
            code.push_str(&generate_structured_binding_specs(ir));
        }
        code.push_str(&generate_include_guards_end(std));

        Ok(code)
    }

    // Everything below delegates to the C++11 generator - the per-method
    // bodies are standard-agnostic.

    fn generate_class_declaration(
        &self,
        code: &mut String,
        struct_def: &StructDef,
        ir: &CodegenIR,
        config: &CodegenConfig,
    ) {
        emit_class_declaration_cpp11_or_later(self, code, struct_def, ir, config);
    }

    fn generate_method_implementations(
        &self,
        code: &mut String,
        struct_def: &StructDef,
        ir: &CodegenIR,
        config: &CodegenConfig,
    ) {
        Cpp11Generator.generate_method_implementations(code, struct_def, ir, config);
    }

    fn generate_destructor(
        &self,
        code: &mut String,
        class_name: &str,
        c_type_name: &str,
        needs_destructor: bool,
    ) {
        Cpp11Generator.generate_destructor(code, class_name, c_type_name, needs_destructor);
    }

    fn generate_copy_move_semantics(
        &self,
        code: &mut String,
        class_name: &str,
        c_type_name: &str,
        is_copy: bool,
        needs_destructor: bool,
    ) {
        Cpp11Generator.generate_copy_move_semantics(
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
        Cpp11Generator.generate_vec_methods(code, struct_def, config);
    }

    fn generate_string_methods(
        &self,
        code: &mut String,
        struct_def: &StructDef,
        config: &CodegenConfig,
    ) {
        Cpp11Generator.generate_string_methods(code, struct_def, config);
    }

    fn generate_option_methods(
        &self,
        code: &mut String,
        struct_def: &StructDef,
        ir: &CodegenIR,
        config: &CodegenConfig,
    ) {
        Cpp11Generator.generate_option_methods(code, struct_def, ir, config);
    }

    fn generate_result_methods(
        &self,
        code: &mut String,
        struct_def: &StructDef,
        ir: &CodegenIR,
        config: &CodegenConfig,
    ) {
        Cpp11Generator.generate_result_methods(code, struct_def, ir, config);
    }
}
