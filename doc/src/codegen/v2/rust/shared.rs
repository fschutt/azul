//! Shared utilities for Rust code generation
//!
//! This module contains common code used by both static and dynamic binding generators.

use std::collections::BTreeSet;

use crate::codegen::v2::config::*;
use crate::codegen::v2::generator::CodeBuilder;
use crate::codegen::v2::ir::*;

// ============================================================================
// GL Type Aliases
// ============================================================================

/// Generate standard GL type aliases used in the API
pub fn generate_gl_type_aliases(builder: &mut CodeBuilder) {
    builder.line("// --- GL Type Aliases ---");
    builder.line("pub type GLenum = u32;");
    builder.line("pub type GLboolean = u8;");
    builder.line("pub type GLbitfield = u32;");
    builder.line("pub type GLbyte = i8;");
    builder.line("pub type GLshort = i16;");
    builder.line("pub type GLint = i32;");
    builder.line("pub type GLsizei = i32;");
    builder.line("pub type GLubyte = u8;");
    builder.line("pub type GLushort = u16;");
    builder.line("pub type GLuint = u32;");
    builder.line("pub type GLfloat = f32;");
    builder.line("pub type GLclampf = f32;");
    builder.line("pub type GLdouble = f64;");
    builder.line("pub type GLclampd = f64;");
    builder.line("pub type GLintptr = isize;");
    builder.line("pub type GLsizeiptr = isize;");
    builder.line("pub type GLint64 = i64;");
    builder.line("pub type GLuint64 = u64;");
    builder.blank();
}

// ============================================================================
// Type Generation
// ============================================================================

/// Generate a type alias definition
pub fn generate_type_alias(
    builder: &mut CodeBuilder,
    type_alias: &TypeAliasDef,
    config: &CodegenConfig,
) {
    let name = config.apply_prefix(&type_alias.name);

    // Target types should also be prefixed if they're not primitives
    let target_base = if is_primitive_type(&type_alias.target) || type_alias.target.contains("::") {
        type_alias.target.clone()
    } else {
        config.apply_prefix(&type_alias.target)
    };

    // Build the full target type including generic arguments
    let target = if type_alias.generic_args.is_empty() {
        target_base
    } else {
        // Apply prefix to each generic argument
        let prefixed_args: Vec<String> = type_alias
            .generic_args
            .iter()
            .map(|arg| {
                if is_primitive_type(arg) || arg.contains("::") {
                    arg.clone()
                } else {
                    config.apply_prefix(arg)
                }
            })
            .collect();
        format!("{}<{}>", target_base, prefixed_args.join(", "))
    };

    builder.line(&format!("pub type {} = {};", name, target));
    builder.blank();
}

/// Generate a callback typedef (function pointer type)
pub fn generate_callback_typedef(
    builder: &mut CodeBuilder,
    callback: &CallbackTypedefDef,
    config: &CodegenConfig,
) {
    let name = config.apply_prefix(&callback.name);

    if config.callback_typedef_use_external {
        if let Some(ref external) = callback.external_path {
            builder.line(&format!("pub type {} = {};", name, external));
            builder.blank();
            return;
        }
    }

    // Generate function pointer signature
    let args: Vec<String> = callback
        .args
        .iter()
        .map(|arg| {
            let type_name = config.apply_prefix(&arg.type_name);
            let ref_prefix = match arg.ref_kind {
                ArgRefKind::Owned => "",
                ArgRefKind::Ref => "&",
                ArgRefKind::RefMut => "&mut ",
                ArgRefKind::Ptr => "*const ",
                ArgRefKind::PtrMut => "*mut ",
            };
            format!("{}{}", ref_prefix, type_name)
        })
        .collect();

    let return_str = callback
        .return_type
        .as_ref()
        .map(|r| format!(" -> {}", config.apply_prefix(r)))
        .unwrap_or_default();

    builder.line(&format!(
        "pub type {} = extern \"C\" fn({}){};",
        name,
        args.join(", "),
        return_str
    ));
    builder.blank();
}

/// Generate a struct definition
pub fn generate_struct(builder: &mut CodeBuilder, struct_def: &StructDef, config: &CodegenConfig) {
    let name = config.apply_prefix(&struct_def.name);

    // Add generic parameters if present
    let generics = if struct_def.generic_params.is_empty() {
        String::new()
    } else {
        format!("<{}>", struct_def.generic_params.join(", "))
    };
    let full_name = format!("{}{}", name, generics);

    // Doc comment
    if !struct_def.doc.is_empty() {
        for doc_line in &struct_def.doc {
            builder.line(&format!("/// {}", doc_line));
        }
    } else {
        builder.line(&format!("/// `{}` struct", name));
    }

    // Repr attribute
    if let Some(ref repr) = struct_def.repr {
        builder.line(&format!("#[repr({})]", repr));
    } else {
        builder.line("#[repr(C)]");
    }

    // Struct definition
    if struct_def.fields.is_empty() {
        builder.line(&format!("pub struct {};", full_name));
    } else {
        builder.line(&format!("pub struct {} {{", full_name));
        builder.indent();
        for field in &struct_def.fields {
            let field_type = format_field_type(&field.type_name, &field.ref_kind, config);
            let visibility = if field.is_public {
                "pub "
            } else {
                "pub(crate) "
            };
            builder.line(&format!("{}{}: {},", visibility, field.name, field_type));
        }
        builder.dedent();
        builder.line("}");
    }
    builder.blank();
}

/// Generate an enum definition
pub fn generate_enum(builder: &mut CodeBuilder, enum_def: &EnumDef, config: &CodegenConfig) {
    let name = config.apply_prefix(&enum_def.name);

    // Add generic parameters if present
    let generics = if enum_def.generic_params.is_empty() {
        String::new()
    } else {
        format!("<{}>", enum_def.generic_params.join(", "))
    };
    let full_name = format!("{}{}", name, generics);

    // Doc comment
    if !enum_def.doc.is_empty() {
        for doc_line in &enum_def.doc {
            builder.line(&format!("/// {}", doc_line));
        }
    } else {
        builder.line(&format!("/// `{}` enum", name));
    }

    // Repr attribute - automatically determine based on whether enum has variant data
    if enum_def.is_union {
        builder.line("#[repr(C, u8)]");
    } else {
        builder.line("#[repr(C)]");
    }

    // Enum definition
    builder.line(&format!("pub enum {} {{", full_name));
    builder.indent();
    for variant in &enum_def.variants {
        match &variant.kind {
            EnumVariantKind::Unit => {
                builder.line(&format!("{},", variant.name));
            }
            EnumVariantKind::Tuple(types) => {
                let types_str: Vec<String> = types.iter().map(|(t, rk)| format_field_type(t, rk, config)).collect();
                builder.line(&format!("{}({}),", variant.name, types_str.join(", ")));
            }
            EnumVariantKind::Struct(fields) => {
                builder.line(&format!("{} {{", variant.name));
                builder.indent();
                for field in fields {
                    let field_type = config.apply_prefix(&field.type_name);
                    builder.line(&format!("{}: {},", field.name, field_type));
                }
                builder.dedent();
                builder.line("},");
            }
        }
    }
    builder.dedent();
    builder.line("}");
    builder.blank();
}

/// Format a field type with its reference kind
pub fn format_field_type(
    type_name: &str,
    ref_kind: &FieldRefKind,
    config: &CodegenConfig,
) -> String {
    let prefixed = config.apply_prefix(type_name);
    match ref_kind {
        FieldRefKind::Owned => prefixed,
        FieldRefKind::Ref => format!("&{}", prefixed),
        FieldRefKind::RefMut => format!("&mut {}", prefixed),
        FieldRefKind::Ptr => format!("*const {}", prefixed),
        FieldRefKind::PtrMut => format!("*mut {}", prefixed),
        FieldRefKind::Boxed => format!("Box<{}>", prefixed),
        FieldRefKind::OptionBoxed => format!("Option<Box<{}>>", prefixed),
    }
}

/// Generate all type definitions (aliases, callbacks, structs, enums)
pub fn generate_types(builder: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    builder.line("// --- Type Definitions ---");
    builder.blank();

    // Type aliases first
    for type_alias in &ir.type_aliases {
        if !config.should_include_type(&type_alias.name) {
            continue;
        }
        generate_type_alias(builder, type_alias, config);
    }

    // Callback typedefs
    for callback in &ir.callback_typedefs {
        if !config.should_include_type(&callback.name) {
            continue;
        }
        generate_callback_typedef(builder, callback, config);
    }

    // Structs
    for struct_def in &ir.structs {
        if !config.should_include_type(&struct_def.name) {
            continue;
        }
        generate_struct(builder, struct_def, config);
    }

    // Enums
    for enum_def in &ir.enums {
        if !config.should_include_type(&enum_def.name) {
            continue;
        }
        generate_enum(builder, enum_def, config);
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Check if a type name is a Rust primitive
pub fn is_primitive_type(type_name: &str) -> bool {
    matches!(
        type_name,
        "bool"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "f32"
            | "f64"
            | "char"
            | "()"
            | "c_void"
            | "c_int"
            | "c_uint"
            | "c_long"
            | "c_ulong"
            | "c_char"
            | "c_uchar" // NOTE: "String" is NOT a primitive - it's AzString in Azul
    )
}
