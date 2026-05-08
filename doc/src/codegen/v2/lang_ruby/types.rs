//! Ruby FFI type emission.
//!
//! Translates IR types into:
//! - simple/unit enums      → `module ENUM_NAME; FOO = 0; BAR = 1; end`
//! - structs                → `class AzFoo < FFI::Struct; layout :x, :int, ...; end`
//! - tagged unions          → `class AzFoo < FFI::Struct; layout :tag, :int, :payload, AzFooPayload; end`
//!                            `class AzFooPayload < FFI::Union; layout :variant1, ...; end`
//! - callback typedefs      → `callback :az_foo_cb, [:pointer, :int], :pointer`
//!
//! Filtering: skips types with `TypeCategory` Recursive / VecRef / GenericTemplate /
//! DestructorOrClone / CallbackTypedef-as-struct. Skipped types receive a
//! `# SKIPPED: <reason>` comment line in the output.

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{
    CallbackTypedefDef, CodegenIR, EnumDef, EnumVariantKind, FieldDef, FieldRefKind, StructDef,
    TypeCategory,
};

// ============================================================================
// Public API
// ============================================================================

/// Emit `class AzFoo < FFI::Struct; end` / `class AzFoo < FFI::Union; end`
/// forward declarations so subsequent layouts can reference any type in any
/// order (FFI requires the class to exist before using `Foo.by_value`).
pub fn emit_forward_declarations(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) {
    builder.line("# --- Forward declarations -------------------------------------");
    for s in &ir.structs {
        if !should_emit_struct(s, config) {
            continue;
        }
        let name = config.apply_prefix(&s.name);
        builder.line(&format!("class {} < FFI::Struct; end", name));
    }
    for e in &ir.enums {
        if !should_emit_enum(e, config) {
            continue;
        }
        if !e.is_union {
            continue;
        }
        let name = config.apply_prefix(&e.name);
        builder.line(&format!("class {} < FFI::Struct; end", name));
        builder.line(&format!("class {}Payload < FFI::Union; end", name));
    }
    builder.blank();
}

/// Emit unit enums as Ruby constant modules.
/// Ruby has no native enum type, so we use `module Foo; A = 0; B = 1; end`,
/// which is the idiomatic FFI-gem pattern.
pub fn emit_simple_enums(builder: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    builder.line("# --- Unit enums (constants) -----------------------------------");
    for e in &ir.enums {
        if !should_emit_enum(e, config) {
            continue;
        }
        if e.is_union {
            continue;
        }
        let name = config.apply_prefix(&e.name);
        builder.line(&format!("module {}", name));
        builder.indent();
        for (idx, v) in e.variants.iter().enumerate() {
            let const_name = ruby_const_name(&v.name);
            builder.line(&format!("{} = {}", const_name, idx));
        }
        builder.dedent();
        builder.line("end");
    }
    builder.blank();
}

/// Emit `callback :name, [args], :return` declarations for every
/// callback typedef. These give us idiomatic function-pointer types.
pub fn emit_callback_typedefs(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) {
    builder.line("# --- Callback typedefs ----------------------------------------");
    for cb in &ir.callback_typedefs {
        emit_callback_typedef(builder, cb, config);
    }
    builder.blank();
}

/// Emit `class AzFoo; layout(:field, :type, ...); end` for every struct.
pub fn emit_struct_layouts(builder: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    builder.line("# --- Struct layouts -------------------------------------------");
    for s in &ir.structs {
        if !should_emit_struct(s, config) {
            emit_skip_marker(builder, &s.name, &skip_reason_struct(s));
            continue;
        }
        emit_struct_layout(builder, s, config, ir);
    }
    builder.blank();
}

/// Emit tagged unions: a wrapper FFI::Struct with `:tag` + `:payload` and a
/// matching FFI::Union holding each variant's payload struct.
pub fn emit_tagged_unions(builder: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    builder.line("# --- Tagged unions --------------------------------------------");
    for e in &ir.enums {
        if !should_emit_enum(e, config) {
            emit_skip_marker(builder, &e.name, &skip_reason_enum(e));
            continue;
        }
        if !e.is_union {
            continue;
        }
        emit_tagged_union(builder, e, config, ir);
    }
    builder.blank();
}

// ============================================================================
// Filtering / skip-reason helpers
// ============================================================================

pub(crate) fn should_emit_struct(s: &StructDef, config: &CodegenConfig) -> bool {
    if !config.should_include_type(&s.name) {
        return false;
    }
    if !s.generic_params.is_empty() {
        return false;
    }
    !matches!(
        s.category,
        TypeCategory::Recursive
            | TypeCategory::VecRef
            | TypeCategory::GenericTemplate
            | TypeCategory::DestructorOrClone
    )
}

pub(crate) fn should_emit_enum(e: &EnumDef, config: &CodegenConfig) -> bool {
    if !config.should_include_type(&e.name) {
        return false;
    }
    if !e.generic_params.is_empty() {
        return false;
    }
    !matches!(
        e.category,
        TypeCategory::Recursive
            | TypeCategory::VecRef
            | TypeCategory::GenericTemplate
            | TypeCategory::DestructorOrClone
    )
}

fn skip_reason_struct(s: &StructDef) -> String {
    if !s.generic_params.is_empty() {
        return format!("generic struct {}<{}>", s.name, s.generic_params.join(", "));
    }
    format!("category={}", s.category.description())
}

fn skip_reason_enum(e: &EnumDef) -> String {
    if !e.generic_params.is_empty() {
        return format!("generic enum {}<{}>", e.name, e.generic_params.join(", "));
    }
    format!("category={}", e.category.description())
}

fn emit_skip_marker(builder: &mut CodeBuilder, name: &str, reason: &str) {
    builder.line(&format!("# SKIPPED: {} ({})", name, reason));
}

// ============================================================================
// Struct emission
// ============================================================================

fn emit_struct_layout(
    builder: &mut CodeBuilder,
    s: &StructDef,
    config: &CodegenConfig,
    ir: &CodegenIR,
) {
    let name = config.apply_prefix(&s.name);

    if s.fields.is_empty() {
        // FFI requires a non-empty layout. Use a single dummy byte to keep the
        // ABI predictable on the Ruby side.
        builder.line(&format!("class {}", name));
        builder.indent();
        builder.line("layout :_dummy, :uint8");
        builder.dedent();
        builder.line("end");
        return;
    }

    builder.line(&format!("class {}", name));
    builder.indent();
    builder.line("layout(");
    builder.indent();
    let last_idx = s.fields.len() - 1;
    for (i, field) in s.fields.iter().enumerate() {
        let trailing = if i == last_idx { "" } else { "," };
        let ruby_type = field_to_ruby_ffi_type(field, config, ir);
        let field_name = ruby_field_name(&field.name);
        builder.line(&format!(":{}, {}{}", field_name, ruby_type, trailing));
    }
    builder.dedent();
    builder.line(")");
    builder.dedent();
    builder.line("end");
}

// ============================================================================
// Tagged union emission
// ============================================================================

fn emit_tagged_union(
    builder: &mut CodeBuilder,
    e: &EnumDef,
    config: &CodegenConfig,
    ir: &CodegenIR,
) {
    let name = config.apply_prefix(&e.name);
    let payload_name = format!("{}Payload", name);

    // Tag constants module
    builder.line(&format!("module {}_Tag", name));
    builder.indent();
    for (idx, v) in e.variants.iter().enumerate() {
        let const_name = ruby_const_name(&v.name);
        builder.line(&format!("{} = {}", const_name, idx));
    }
    builder.dedent();
    builder.line("end");

    // Generate one tiny FFI::Struct per variant payload (so the union can
    // reference a named layout). Variants without payload get a single dummy
    // byte so FFI accepts them.
    for v in &e.variants {
        let v_struct_name = format!("{}Variant{}", name, v.name);
        builder.line(&format!("class {} < FFI::Struct", v_struct_name));
        builder.indent();
        match &v.kind {
            EnumVariantKind::Unit => {
                builder.line("layout :_dummy, :uint8");
            }
            EnumVariantKind::Tuple(types) if !types.is_empty() => {
                builder.line("layout(");
                builder.indent();
                let last = types.len() - 1;
                for (i, (ty, ref_kind)) in types.iter().enumerate() {
                    let trailing = if i == last { "" } else { "," };
                    let ruby_ty = type_with_ref_to_ruby(ty, *ref_kind, config, ir, false);
                    let field_name = if types.len() == 1 {
                        "payload".to_string()
                    } else {
                        format!("payload_{}", i)
                    };
                    builder.line(&format!(":{}, {}{}", field_name, ruby_ty, trailing));
                }
                builder.dedent();
                builder.line(")");
            }
            EnumVariantKind::Tuple(_) => {
                builder.line("layout :_dummy, :uint8");
            }
            EnumVariantKind::Struct(fields) if !fields.is_empty() => {
                builder.line("layout(");
                builder.indent();
                let last = fields.len() - 1;
                for (i, f) in fields.iter().enumerate() {
                    let trailing = if i == last { "" } else { "," };
                    let ruby_ty = field_to_ruby_ffi_type(f, config, ir);
                    let field_name = ruby_field_name(&f.name);
                    builder.line(&format!(":{}, {}{}", field_name, ruby_ty, trailing));
                }
                builder.dedent();
                builder.line(")");
            }
            EnumVariantKind::Struct(_) => {
                builder.line("layout :_dummy, :uint8");
            }
        }
        builder.dedent();
        builder.line("end");
    }

    // Payload union: a member per variant
    builder.line(&format!("class {}", payload_name));
    builder.indent();
    builder.line("layout(");
    builder.indent();
    let last_idx = e.variants.len() - 1;
    for (i, v) in e.variants.iter().enumerate() {
        let trailing = if i == last_idx { "" } else { "," };
        let v_struct_name = format!("{}Variant{}", name, v.name);
        let field = ruby_field_name(&v.name);
        builder.line(&format!(":{}, {}{}", field, v_struct_name, trailing));
    }
    builder.dedent();
    builder.line(")");
    builder.dedent();
    builder.line("end");

    // Outer struct: tag + payload
    builder.line(&format!("class {}", name));
    builder.indent();
    builder.line("layout(");
    builder.indent();
    builder.line(":tag, :int,");
    builder.line(&format!(":payload, {}", payload_name));
    builder.dedent();
    builder.line(")");
    builder.dedent();
    builder.line("end");
}

// ============================================================================
// Callback typedef emission
// ============================================================================

fn emit_callback_typedef(
    builder: &mut CodeBuilder,
    cb: &CallbackTypedefDef,
    config: &CodegenConfig,
) {
    let name = ruby_symbol_for_callback(&config.apply_prefix(&cb.name));

    let mut parts: Vec<String> = Vec::with_capacity(cb.args.len());
    for arg in &cb.args {
        // Callbacks always pass non-trivially-sized things by pointer in Ruby's
        // FFI. We err on the side of `:pointer` for any non-primitive type.
        parts.push(arg_callback_ffi_type(&arg.type_name));
    }

    let ret = match &cb.return_type {
        Some(t) => arg_callback_ffi_type(t),
        None => ":void".to_string(),
    };

    builder.line(&format!(
        "callback :{}, [{}], {}",
        name,
        parts.join(", "),
        ret
    ));
}

/// Conservative FFI type for a callback argument: primitives map to their
/// natural FFI type, everything else falls back to `:pointer` because the C
/// ABI passes structs through stack/registers in ways `ffi` cannot mirror
/// without `.by_value` (which only works on already-attached FFI::Struct).
fn arg_callback_ffi_type(rust_type: &str) -> String {
    let trimmed = rust_type.trim();
    if trimmed.starts_with('*') || trimmed.starts_with('&') {
        return ":pointer".to_string();
    }
    if let Some(p) = primitive_to_ffi_symbol(trimmed) {
        return p.to_string();
    }
    ":pointer".to_string()
}

// ============================================================================
// Type translation: Rust IR → Ruby FFI symbol/class
// ============================================================================

/// Turn a `FieldDef` into the right Ruby FFI type token.
pub(crate) fn field_to_ruby_ffi_type(
    field: &FieldDef,
    config: &CodegenConfig,
    ir: &CodegenIR,
) -> String {
    type_with_ref_to_ruby(&field.type_name, field.ref_kind, config, ir, true)
}

/// Convert a (type_name, ref_kind) pair to a Ruby FFI type token.
///
/// `prefer_by_value` controls whether a non-primitive plain field becomes
/// `Foo.by_value` (true, structs) or stays as an FFI symbol (false).
pub(crate) fn type_with_ref_to_ruby(
    type_name: &str,
    ref_kind: FieldRefKind,
    config: &CodegenConfig,
    ir: &CodegenIR,
    prefer_by_value: bool,
) -> String {
    // Reference / pointer / boxed types: always `:pointer` on the Ruby side.
    match ref_kind {
        FieldRefKind::Ref
        | FieldRefKind::RefMut
        | FieldRefKind::Ptr
        | FieldRefKind::PtrMut
        | FieldRefKind::Boxed
        | FieldRefKind::OptionBoxed => return ":pointer".to_string(),
        FieldRefKind::Owned => {}
    }

    let trimmed = type_name.trim();

    // Array types: `[u8; 4]`
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        if let Some(semi) = trimmed[1..trimmed.len() - 1].rfind(';') {
            let elem = trimmed[1..1 + semi].trim();
            let count = trimmed[1 + semi + 1..trimmed.len() - 1].trim();
            let elem_ffi = primitive_to_ffi_symbol(elem)
                .map(str::to_string)
                .unwrap_or_else(|| format!("{}.by_value", config.apply_prefix(elem)));
            return format!("[{}, {}]", elem_ffi, count);
        }
    }

    // Raw pointer literal in the type string ("*const u8" / "*mut Foo").
    if trimmed.starts_with('*') || trimmed.starts_with('&') {
        return ":pointer".to_string();
    }

    // Primitives.
    if let Some(p) = primitive_to_ffi_symbol(trimmed) {
        return p.to_string();
    }

    // Callback typedef: use the Ruby `callback :name` symbol.
    if ir.callback_typedefs.iter().any(|c| c.name == trimmed) {
        let sym = ruby_symbol_for_callback(&config.apply_prefix(trimmed));
        return format!(":{}", sym);
    }

    // Unit (non-union) enum: stored as :int.
    if let Some(en) = ir.enums.iter().find(|e| e.name == trimmed) {
        if !en.is_union {
            return ":int".to_string();
        }
    }

    // Anything else is a struct / tagged union: pass by value.
    let prefixed = config.apply_prefix(trimmed);
    if prefer_by_value {
        format!("{}.by_value", prefixed)
    } else {
        format!("{}.by_value", prefixed)
    }
}

/// Map a Rust primitive type string to a Ruby FFI symbol.
fn primitive_to_ffi_symbol(rust_type: &str) -> Option<&'static str> {
    Some(match rust_type {
        "bool" => ":bool",
        "u8" => ":uint8",
        "i8" => ":int8",
        "u16" => ":uint16",
        "i16" => ":int16",
        "u32" => ":uint32",
        "i32" => ":int32",
        "u64" => ":uint64",
        "i64" => ":int64",
        "usize" => ":size_t",
        "isize" => ":ssize_t",
        "f32" => ":float",
        "f64" => ":double",
        "char" => ":char",
        "c_char" => ":char",
        "c_uchar" => ":uchar",
        "c_short" => ":short",
        "c_ushort" => ":ushort",
        "c_int" => ":int",
        "c_uint" => ":uint",
        "c_long" => ":long",
        "c_ulong" => ":ulong",
        "c_longlong" => ":long_long",
        "c_ulonglong" => ":ulong_long",
        "c_float" => ":float",
        "c_double" => ":double",
        "c_void" | "()" | "void" => ":void",
        _ => return None,
    })
}

// ============================================================================
// Identifier helpers
// ============================================================================

/// Ruby reserved words & FFI built-ins that collide with field names.
const RUBY_RESERVED: &[&str] = &[
    "alias", "and", "begin", "break", "case", "class", "def", "defined", "do", "else", "elsif",
    "end", "ensure", "false", "for", "if", "in", "module", "next", "nil", "not", "or", "redo",
    "rescue", "retry", "return", "self", "super", "then", "true", "undef", "unless", "until",
    "when", "while", "yield",
];

pub(crate) fn ruby_field_name(field: &str) -> String {
    if RUBY_RESERVED.contains(&field) {
        format!("{}_", field)
    } else {
        field.to_string()
    }
}

pub(crate) fn ruby_const_name(variant: &str) -> String {
    // Variant names from api.json are CamelCase (None, Some, RefreshDom).
    // Ruby constants must start with an uppercase letter, which matches.
    // Enforce that here.
    let mut chars = variant.chars();
    match chars.next() {
        Some(c) if c.is_ascii_uppercase() => variant.to_string(),
        Some(c) => format!("{}{}", c.to_ascii_uppercase(), chars.as_str()),
        None => "Unknown".to_string(),
    }
}

/// Convert a CamelCase callback name (e.g. "AzLayoutCallbackType") into the
/// Ruby symbol used in `callback :name, [...]` (e.g. "az_layout_callback_type").
pub(crate) fn ruby_symbol_for_callback(name: &str) -> String {
    snake_case(name)
}

/// Naive CamelCase → snake_case (sufficient for AzFooBar style identifiers).
pub(crate) fn snake_case(input: &str) -> String {
    let mut out = String::with_capacity(input.len() + 4);
    for (i, c) in input.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i != 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}
