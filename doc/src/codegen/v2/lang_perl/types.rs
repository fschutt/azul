//! Perl FFI type emission.
//!
//! Translates IR types into:
//!
//! - simple (unit) enums  → `package Azul::AzFoo { sub Bar () { 0 } sub Baz () { 1 } }`
//! - structs              → `package Azul::AzFoo { use FFI::Platypus::Record; record_layout_1($Azul::ffi, ...) }`
//!                          plus `$ffi->type('record(Azul::AzFoo)' => 'Azul::AzFoo');`
//! - tagged unions        → outer record `{ tag => 'sint32', payload => "opaque[N]" }`
//!                          (a fixed-size byte blob; per-variant accessor methods are
//!                          provided by the wrapper layer in `wrappers.rs`).
//!
//! Filtering: skips types with `TypeCategory` Recursive / VecRef / GenericTemplate /
//! DestructorOrClone. Skipped types receive a `# SKIPPED: <reason>` comment line.
//!
//! Type translation is intentionally conservative:
//! - non-primitive value types pass via `'record(Azul::AzFoo)'` so layouts can
//!   nest naturally;
//! - any pointer / reference becomes `'opaque'` (FFI::Platypus's word for an
//!   unmanaged void*);
//! - arrays of primitives become `"<elem>[<count>]"` (FFI::Platypus's array
//!   spec syntax inside record_layout_1).
//!
//! All generated string literals use single quotes so Perl never interpolates
//! `$var` / `@list` accidentally — every `$Azul::ffi` reference is intentional.

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{
    CallbackTypedefDef, CodegenIR, EnumDef, FieldDef, FieldRefKind, StructDef, TypeCategory,
};

// ============================================================================
// Public API
// ============================================================================

/// Empty `package Azul::AzFoo {}` blocks so the type names exist before any
/// `record_layout_1` references them. FFI::Platypus accepts the type alias
/// registration even before the package has a record layout, so this also
/// pre-registers `'record(Azul::AzFoo)'` -> `'Azul::AzFoo'`.
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
        builder.line(&format!("package Azul::{} {{ }}", name));
    }
    for e in &ir.enums {
        if !should_emit_enum(e, config) {
            continue;
        }
        if !e.is_union {
            continue;
        }
        let name = config.apply_prefix(&e.name);
        builder.line(&format!("package Azul::{} {{ }}", name));
    }
    builder.blank();
}

/// Emit unit enums as Perl packages whose constants are constant-fold-able
/// subs. The `() : const` prototype tells perl to inline them.
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
        builder.line(&format!("package Azul::{} {{", name));
        builder.indent();
        for (idx, v) in e.variants.iter().enumerate() {
            let const_name = perl_const_name(&v.name);
            builder.line(&format!("sub {} () {{ {} }}", const_name, idx));
        }
        builder.dedent();
        builder.line("}");
    }
    builder.blank();
}

/// Emit `callback typedef` shims. FFI::Platypus represents function-pointer
/// callbacks via `$ffi->type('(args)->ret' => 'name')`; the wrapper layer
/// handles caller-side `$ffi->closure(sub { ... })` wrapping.
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

/// Emit `record_layout_1` blocks for each emittable struct.
pub fn emit_struct_layouts(builder: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    builder.line("# --- Struct layouts (FFI::Platypus::Record) -------------------");
    for s in &ir.structs {
        if !should_emit_struct(s, config) {
            emit_skip_marker(builder, &s.name, &skip_reason_struct(s));
            continue;
        }
        emit_struct_layout(builder, s, config, ir);
    }
    builder.blank();
}

/// Emit struct AND tagged-union records interleaved by their shared
/// `sort_order`. FFI::Platypus::Record evaluates `record_layout_1`
/// at module load and requires every referenced field type to be
/// fully defined at that moment — emitting a struct that nests
/// another struct whose `record_layout_1` hasn't yet run produces:
///   Use of uninitialized value $align in numeric gt (>)
///   Illegal modulus zero at FFI/Platypus/Record.pm line 96.
pub fn emit_records_in_sort_order(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) {
    builder.line("# --- Struct + tagged-union layouts (topologically ordered) ----");
    enum Item<'a> {
        Struct(&'a StructDef),
        Union(&'a EnumDef),
        Mono(&'a super::super::ir::TypeAliasDef),
    }
    let mut items: Vec<(usize, Item)> = Vec::new();
    for s in &ir.structs {
        items.push((s.sort_order, Item::Struct(s)));
    }
    for e in &ir.enums {
        if e.is_union {
            items.push((e.sort_order, Item::Union(e)));
        }
    }
    // Monomorphized type aliases (PhysicalSizeU32, Option*<T>, *Value
    // family) are referenced by other records as fields and must be
    // declared in topological order alongside structs/unions.
    for ta in &ir.type_aliases {
        if ta.monomorphized_def.is_none() {
            continue;
        }
        if !config.should_include_type(&ta.name) {
            continue;
        }
        items.push((ta.sort_order, Item::Mono(ta)));
    }
    items.sort_by_key(|(ord, _)| *ord);
    for (_, item) in items {
        match item {
            Item::Struct(s) => {
                if !should_emit_struct(s, config) {
                    emit_skip_marker(builder, &s.name, &skip_reason_struct(s));
                    continue;
                }
                emit_struct_layout(builder, s, config, ir);
            }
            Item::Union(e) => {
                if !should_emit_enum(e, config) {
                    emit_skip_marker(builder, &e.name, &skip_reason_enum(e));
                    continue;
                }
                emit_tagged_union(builder, e, config);
            }
            Item::Mono(ta) => emit_monomorphized_alias(builder, ta, config, ir),
        }
    }
    builder.blank();
}

/// Emit a monomorphized type-alias instantiation (PhysicalSizeU32,
/// OptionU32, OptionF32, *Value family). Same shape as
/// emit_struct_layout but driven from `monomorphized_def.kind`.
fn emit_monomorphized_alias(
    builder: &mut CodeBuilder,
    ta: &super::super::ir::TypeAliasDef,
    config: &CodegenConfig,
    ir: &CodegenIR,
) {
    use super::super::ir::MonomorphizedKind;
    let name = config.apply_prefix(&ta.name);
    let pkg = format!("Azul::{}", name);
    let Some(ref mono) = ta.monomorphized_def else {
        return;
    };
    builder.line(&format!("package {} {{", pkg));
    builder.indent();
    builder.line("use FFI::Platypus::Record;");
    match &mono.kind {
        MonomorphizedKind::SimpleEnum { .. } => {
            // Same shape as a unit enum: stored as sint32.
            builder.line("record_layout_1($Azul::ffi,");
            builder.indent();
            builder.line("'sint32' => 'tag',");
            builder.dedent();
            builder.line(");");
        }
        MonomorphizedKind::Struct { fields } => {
            if fields.is_empty() {
                builder.line("record_layout_1($Azul::ffi,");
                builder.indent();
                builder.line("'uint8' => '_padding',");
                builder.dedent();
                builder.line(");");
            } else {
                builder.line("record_layout_1($Azul::ffi,");
                builder.indent();
                for f in fields {
                    let field_name = perl_field_name(&f.name);
                    let referenced = nested_struct_name(f, ir);
                    if let Some(inner) = referenced {
                        let inner_alias = config.apply_prefix(&inner);
                        builder.line(&format!(
                            "'string(' . $Azul::ffi->sizeof('{}') . ')' => '{}',",
                            inner_alias, field_name
                        ));
                    } else {
                        let perl_type =
                            type_with_ref_to_perl(&f.type_name, f.ref_kind, config, ir);
                        builder.line(&format!("'{}' => '{}',", perl_type, field_name));
                    }
                }
                builder.dedent();
                builder.line(");");
            }
        }
        MonomorphizedKind::TaggedUnion { .. } => {
            // Tag + conservative 64-byte payload — matches lang_cobol's
            // strategy. The exact variant byte size can't be computed
            // portably without per-platform alignment info.
            builder.line("record_layout_1($Azul::ffi,");
            builder.indent();
            builder.line("'sint32' => 'tag',");
            builder.line("'uint8[64]' => 'payload',");
            builder.dedent();
            builder.line(");");
        }
    }
    builder.dedent();
    builder.line("}");
    builder.line(&format!(
        "$Azul::ffi->type('record({pkg})' => '{alias}');",
        pkg = pkg,
        alias = name
    ));
}

/// Tagged unions: emit a single record with a `tag` integer + an opaque
/// fixed-size blob big enough to hold the largest variant. We can't compute
/// the exact byte size from the IR alone (it depends on host `repr(C)`
/// alignment), so we conservatively emit a generously-sized opaque blob and
/// let FFI::Platypus marshal it through pass-by-value.
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
        emit_tagged_union(builder, e, config);
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
    let pkg = format!("Azul::{}", name);

    builder.line(&format!("package {} {{", pkg));
    builder.indent();
    builder.line("use FFI::Platypus::Record;");

    if s.fields.is_empty() {
        // FFI::Platypus::Record requires at least one field; emit a single
        // padding byte so `record(Azul::AzFoo)` still works.
        builder.line("record_layout_1($Azul::ffi,");
        builder.indent();
        builder.line("'uint8' => '_padding',");
        builder.dedent();
        builder.line(");");
    } else {
        builder.line("record_layout_1($Azul::ffi,");
        builder.indent();
        for field in &s.fields {
            let field_name = perl_field_name(&field.name);
            // FFI::Platypus 2.11 has a known bug: `record_layout_1`
            // rejects nested records (`type not supported`) and the
            // upstream `alignof` returns undef for `record_value`-typed
            // fields. Workaround: for nested-struct fields, emit a
            // runtime-sized opaque `string(N)` buffer where N is
            // resolved from `$Azul::ffi->sizeof('AzInner')` at
            // module load time. Users round-trip raw bytes; preserves
            // the binary layout (which is what callers care about
            // for the C-ABI call) at the cost of named-field access.
            let referenced_struct = nested_struct_name(field, ir);
            if let Some(inner) = referenced_struct {
                let inner_alias = config.apply_prefix(&inner);
                builder.line(&format!(
                    "'string(' . $Azul::ffi->sizeof('{}') . ')' => '{}',",
                    inner_alias, field_name
                ));
            } else {
                let perl_type = field_to_perl_record_type(field, config, ir);
                builder.line(&format!("'{}' => '{}',", perl_type, field_name));
            }
        }
        builder.dedent();
        builder.line(");");
    }

    builder.dedent();
    builder.line("}");
    // Register the type alias so `record(Azul::AzFoo)` resolves in
    // nested layouts. The alias name (right-hand side) must match
    // FFI::Platypus's `[A-Za-z0-9_]` rule — `::` is not legal there —
    // so register the alias as the bare `AzFoo` and pass that name as
    // the field type in other layouts.
    builder.line(&format!(
        "$Azul::ffi->type('record({pkg})' => '{alias}');",
        pkg = pkg,
        alias = name
    ));
}

// ============================================================================
// Tagged union emission
// ============================================================================

fn emit_tagged_union(builder: &mut CodeBuilder, e: &EnumDef, config: &CodegenConfig) {
    let name = config.apply_prefix(&e.name);
    let pkg = format!("Azul::{}", name);

    // Tag constants package: `Azul::AzFoo::Tag`
    builder.line(&format!("package Azul::{}::Tag {{", name));
    builder.indent();
    for (idx, v) in e.variants.iter().enumerate() {
        let const_name = perl_const_name(&v.name);
        builder.line(&format!("sub {} () {{ {} }}", const_name, idx));
    }
    builder.dedent();
    builder.line("}");

    // Outer record: tag + opaque payload blob. We pick a conservative blob
    // size (256 bytes) — large enough for every payload in api.json today
    // and still tiny in absolute terms. Variant accessors live in wrappers.rs.
    builder.line(&format!("package {} {{", pkg));
    builder.indent();
    builder.line("use FFI::Platypus::Record;");
    builder.line("# SKIPPED: per-variant payload accessors (use Azul::FFI::* for raw access)");
    builder.line("record_layout_1($Azul::ffi,");
    builder.indent();
    builder.line("'sint32' => 'tag',");
    builder.line("'uint8[256]' => 'payload',");
    builder.dedent();
    builder.line(");");
    builder.dedent();
    builder.line("}");
    builder.line(&format!(
        "$Azul::ffi->type('record({pkg})' => '{alias}');",
        pkg = pkg,
        alias = name
    ));
}

// ============================================================================
// Callback typedef emission
// ============================================================================

fn emit_callback_typedef(
    builder: &mut CodeBuilder,
    cb: &CallbackTypedefDef,
    config: &CodegenConfig,
) {
    let name = config.apply_prefix(&cb.name);

    let mut parts: Vec<String> = Vec::with_capacity(cb.args.len());
    for arg in &cb.args {
        parts.push(arg_callback_ffi_type(&arg.type_name).to_string());
    }
    let ret = match &cb.return_type {
        Some(t) => arg_callback_ffi_type(t).to_string(),
        None => "void".to_string(),
    };

    // FFI::Platypus closure-typedef syntax: `(args)->ret`.
    builder.line(&format!(
        "$Azul::ffi->type('({})->{}' => '{}');",
        parts.join(","),
        ret,
        name
    ));
}

/// Conservative FFI type for a callback argument.
fn arg_callback_ffi_type(rust_type: &str) -> &'static str {
    let trimmed = rust_type.trim();
    if trimmed.starts_with('*') || trimmed.starts_with('&') {
        return "opaque";
    }
    if let Some(p) = primitive_to_ffi_name(trimmed) {
        return p;
    }
    "opaque"
}

// ============================================================================
// Type translation: Rust IR → Perl FFI::Platypus type name
// ============================================================================

/// Type used inside `record_layout_1` for a struct field.
pub(crate) fn field_to_perl_record_type(
    field: &FieldDef,
    config: &CodegenConfig,
    ir: &CodegenIR,
) -> String {
    type_with_ref_to_perl(&field.type_name, field.ref_kind, config, ir)
}

/// If the field's owned (by-value) type is a struct, tagged union, or
/// monomorphized alias declared by this codegen, return its IR name.
/// Used to decide whether the Perl-side `record_layout_1` line needs
/// the `'string(' . sizeof ... . ')'` opaque-buffer workaround.
fn nested_struct_name(f: &FieldDef, ir: &CodegenIR) -> Option<String> {
    match f.ref_kind {
        FieldRefKind::Ref
        | FieldRefKind::RefMut
        | FieldRefKind::Ptr
        | FieldRefKind::PtrMut
        | FieldRefKind::Boxed
        | FieldRefKind::OptionBoxed => return None,
        FieldRefKind::Owned => {}
    }
    let trimmed = f.type_name.trim();
    if trimmed.starts_with('[') || trimmed.starts_with('*') || trimmed.starts_with('&') {
        return None;
    }
    if primitive_to_ffi_name(trimmed).is_some() {
        return None;
    }
    if let Some(s) = ir.find_struct(trimmed) {
        // Skipped categories (Recursive / VecRef / DestructorOrClone /
        // GenericTemplate) don't get a record_layout_1, so we can't
        // ask sizeof for them. Caller falls through to the regular
        // type path, which lowers to `opaque` for skipped structs.
        if matches!(
            s.category,
            TypeCategory::Recursive
                | TypeCategory::VecRef
                | TypeCategory::DestructorOrClone
                | TypeCategory::GenericTemplate
        ) || !s.generic_params.is_empty()
        {
            return None;
        }
        return Some(trimmed.to_string());
    }
    if let Some(en) = ir.find_enum(trimmed) {
        // Skipped enum categories (DestructorOrClone, Recursive,
        // GenericTemplate) don't get emitted either; fall through so
        // the field becomes opaque.
        if matches!(
            en.category,
            TypeCategory::Recursive
                | TypeCategory::DestructorOrClone
                | TypeCategory::GenericTemplate
        ) || !en.generic_params.is_empty()
        {
            return None;
        }
        // Unit enums map to sint32, not a nested record; tagged unions
        // are by-value records that hit the same FFI::Platypus bug.
        if en.is_union {
            return Some(trimmed.to_string());
        }
    }
    if let Some(ta) = ir.find_type_alias(trimmed) {
        // Simple type aliases (GLint = i32) are NOT nested records;
        // FFI::Platypus accepts them as plain primitives. Only
        // monomorphized aliases (PhysicalSizeU32, *Value family)
        // are record-shaped and need the opaque-buffer workaround.
        if ta.monomorphized_def.is_some() {
            return Some(trimmed.to_string());
        }
    }
    None
}

/// Convert a (type_name, ref_kind) pair to a FFI::Platypus type token
/// suitable for use inside a `record_layout_1` block or as an `attach` arg.
pub(crate) fn type_with_ref_to_perl(
    type_name: &str,
    ref_kind: FieldRefKind,
    config: &CodegenConfig,
    ir: &CodegenIR,
) -> String {
    // References / pointers / boxed -> opaque void*.
    match ref_kind {
        FieldRefKind::Ref
        | FieldRefKind::RefMut
        | FieldRefKind::Ptr
        | FieldRefKind::PtrMut
        | FieldRefKind::Boxed
        | FieldRefKind::OptionBoxed => return "opaque".to_string(),
        FieldRefKind::Owned => {}
    }

    let trimmed = type_name.trim();

    // Array types: `[u8; 4]` → `uint8[4]`.
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        if let Some(semi) = trimmed[1..trimmed.len() - 1].rfind(';') {
            let elem = trimmed[1..1 + semi].trim();
            let count = trimmed[1 + semi + 1..trimmed.len() - 1].trim();
            let elem_ffi = primitive_to_ffi_name(elem)
                .map(str::to_string)
                .unwrap_or_else(|| {
                    // Non-primitive arrays fall back to a byte blob the size
                    // we can't safely infer here; mark them opaque.
                    "uint8".to_string()
                });
            return format!("{}[{}]", elem_ffi, count);
        }
    }

    // Raw pointer literal in the type string.
    if trimmed.starts_with('*') || trimmed.starts_with('&') {
        return "opaque".to_string();
    }

    // Primitives.
    if let Some(p) = primitive_to_ffi_name(trimmed) {
        return p.to_string();
    }

    // Callback typedef: callbacks are function pointers, which Platypus
    // can store as opaque void* in a record. Users construct a typed
    // funptr via `$ffi->closure(...)` and pass that.
    if ir.callback_typedefs.iter().any(|c| c.name == trimmed) {
        return "opaque".to_string();
    }

    // Unit (non-union) enum: stored as :int.
    if let Some(en) = ir.enums.iter().find(|e| e.name == trimmed) {
        // Skipped enum categories → opaque.
        if matches!(
            en.category,
            TypeCategory::Recursive
                | TypeCategory::DestructorOrClone
                | TypeCategory::GenericTemplate
        ) || !en.generic_params.is_empty()
        {
            return "opaque".to_string();
        }
        if !en.is_union {
            return "sint32".to_string();
        }
    }

    // Simple type aliases (GLint = i32, GLuint = u32, etc.): resolve
    // recursively to the target type. The codegen doesn't register a
    // Platypus type for simple aliases, so referring to AzGLint by
    // name raises "unknown type". Only follow plain aliases (no
    // monomorphized_def) — monomorphized aliases ARE registered as
    // records.
    if let Some(ta) = ir.find_type_alias(trimmed) {
        if ta.monomorphized_def.is_none() {
            return type_with_ref_to_perl(&ta.target, ref_kind, config, ir);
        }
    }

    // Struct in a skipped category — no record_layout_1 emitted, so
    // referencing it by name fails. These types are typically heap-
    // allocated handles (`VecDestructor`, `Recursive`, …) that
    // appear in field position only as pointer-shaped data.
    if let Some(s) = ir.find_struct(trimmed) {
        if matches!(
            s.category,
            TypeCategory::Recursive
                | TypeCategory::VecRef
                | TypeCategory::DestructorOrClone
                | TypeCategory::GenericTemplate
        ) || !s.generic_params.is_empty()
        {
            return "opaque".to_string();
        }
    }

    // Anything else is a struct / tagged union: pass by value.
    // Reference the type by its registered alias name (bare `AzFoo`,
    // no `Azul::` prefix and no `record(...)` wrapper) so the type
    // resolves via `$ffi->type` rather than re-deriving the alignof
    // from the package directly — `record(Azul::AzFoo)` re-derivation
    // returns undef alignof when referenced from another `record_layout_1`
    // call site, even after the package's own `record_layout_1` ran.
    let prefixed = config.apply_prefix(trimmed);
    prefixed
}

/// Map a Rust primitive type string to a FFI::Platypus type name (no
/// leading colon — Platypus uses bare strings, unlike Ruby's `:symbol`).
pub(crate) fn primitive_to_ffi_name(rust_type: &str) -> Option<&'static str> {
    Some(match rust_type {
        "bool" => "uint8",
        "u8" => "uint8",
        "i8" => "sint8",
        "u16" => "uint16",
        "i16" => "sint16",
        "u32" => "uint32",
        "i32" => "sint32",
        "u64" => "uint64",
        "i64" => "sint64",
        "usize" => "size_t",
        "isize" => "ssize_t",
        "f32" => "float",
        "f64" => "double",
        "char" => "sint8",
        "c_char" => "sint8",
        "c_uchar" => "uint8",
        "c_short" => "sint16",
        "c_ushort" => "uint16",
        "c_int" => "sint32",
        "c_uint" => "uint32",
        "c_long" => "long",
        "c_ulong" => "unsigned long",
        "c_longlong" => "sint64",
        "c_ulonglong" => "uint64",
        "c_float" => "float",
        "c_double" => "double",
        "c_void" | "()" | "void" => "void",
        _ => return None,
    })
}

// ============================================================================
// Identifier helpers
// ============================================================================

/// Perl reserved words that cannot be used as bareword field/sub names.
/// Most clash with Perl flow-control keywords; collisions get a trailing `_`.
const PERL_RESERVED: &[&str] = &[
    "and", "cmp", "continue", "do", "else", "elsif", "eq", "for", "foreach", "ge", "gt", "if",
    "le", "lt", "ne", "next", "no", "not", "or", "package", "redo", "require", "return", "sub",
    "unless", "until", "use", "while", "x", "xor", "AUTOLOAD", "BEGIN", "DESTROY", "END",
];

pub(crate) fn perl_field_name(field: &str) -> String {
    if PERL_RESERVED.contains(&field) {
        format!("{}_", field)
    } else {
        field.to_string()
    }
}

/// Constants in Perl can be any case, but variant names from api.json are
/// already CamelCase (None, Some, RefreshDom). Force the leading char to
/// upper-case for consistency.
pub(crate) fn perl_const_name(variant: &str) -> String {
    let mut chars = variant.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() => {
            let mut s = String::with_capacity(variant.len());
            s.push(c.to_ascii_uppercase());
            s.push_str(chars.as_str());
            s
        }
        Some(_) | None => format!("V_{}", variant),
    }
}

/// Naive CamelCase → snake_case (sufficient for AzFooBar style identifiers).
/// Used by `wrappers.rs` and `functions.rs` to derive package / method names.
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
