//! Idiomatic Perl class wrappers under `package Azul::<Type>`.
//!
//! Every struct that has a matching `<TypeName>_delete` C function gets a
//! lightweight Perl class that:
//!
//! 1. Stores the underlying opaque pointer in a blessed scalar reference (`bless \$ptr, $class`).
//!    This is the canonical Perl idiom for opaque handles — accessing `$$self` recovers the raw
//!    pointer.
//! 2. Defines a `DESTROY` method (Perl's deterministic refcount-driven finalizer) that calls the
//!    corresponding `Azul::FFI::Az<Type>_delete` when the object is freed.
//! 3. Exposes idiomatic class methods (constructors, static helpers) and instance methods (anything
//!    that takes `&self` / `&mut self`).
//!
//! Method naming: drop the `Az` prefix and the `<TypeName>_` segment, then
//! convert `camelCase` to `snake_case`. So:
//!
//! - `AzApp_create`        → `Azul::App->create`        (static)
//! - `AzApp_run`           → `$app->run`                (instance)
//! - `AzAppConfig_default` → `Azul::AppConfig->default` (static)
//! - `AzDom_addChild`      → `$dom->add_child`          (instance)
//!
//! POD structs without a `_delete` get no wrapper — users instantiate the
//! `Azul::AzFoo` record class directly via `Azul::AzFoo->new(...)`.

use std::collections::BTreeSet;

use super::{
    super::{
        config::CodegenConfig,
        generator::CodeBuilder,
        ir::{ArgRefKind, CodegenIR, FunctionDef, FunctionKind, StructDef, TypeCategory},
    },
    types::{should_emit_struct, snake_case},
};

// ============================================================================
// Public entry point
// ============================================================================

/// Emit Perl wrapper packages for every struct that owns heap memory.
pub fn emit_wrappers(builder: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    builder.line("# ============================================================");
    builder.line("# Idiomatic wrappers (Az prefix dropped). Use these in user code.");
    builder.line("# ============================================================");
    builder.blank();

    emit_runtime_helpers(builder, ir, config);

    let delete_set = collect_delete_targets(ir);

    for s in &ir.structs {
        if !should_emit_struct(s, config) {
            continue;
        }
        if !delete_set.contains(s.name.as_str()) {
            // POD struct — no finalizer required, no wrapper class.
            builder.line(&format!(
                "# (no wrapper for {} -- no _delete; use Azul::{} directly)",
                s.name,
                config.apply_prefix(&s.name)
            ));
            continue;
        }
        emit_class_wrapper(builder, s, ir, config);
        builder.blank();
    }

    // Field accessors for the POD structs, on their record packages
    // (`Azul::AzWindowSize->get_dimensions`): they have no wrapper class.
    for s in &ir.structs {
        if !should_emit_struct(s, config) || delete_set.contains(s.name.as_str()) {
            continue;
        }
        let mut body = CodeBuilder::new("    ");
        body.indent();
        let n = emit_field_accessors(&mut body, s, ir, config, Receiver::Record, &BTreeSet::new());
        if n == 0 {
            continue;
        }
        body.dedent();
        builder.line(&format!(
            "package Azul::{} {{ # field accessors",
            config.apply_prefix(&s.name)
        ));
        builder.raw(&body.finish());
        builder.line("}");
        builder.blank();
    }
}

/// The `package Azul` helpers the wrappers and field accessors share.
///
/// A wrapper (`Azul::<Type>`, marked by `_az_wrapper`) holds the RECORD
/// object libazul returned (`$$self`): a blessed scalar ref whose string is
/// the value's bytes in C layout. Every C parameter that is a pointer
/// (`&self`, `&T`, a field's address) gets the address of those bytes -
/// never the record object itself, which FFI::Platypus would turn into the
/// address of the Perl SV.
fn emit_runtime_helpers(builder: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    use super::super::c_layout;
    let string = ir
        .structs
        .iter()
        .find(|s| s.category == TypeCategory::String);
    // AzString = { vec: U8Vec { ptr, len, ... } }: where the byte pointer
    // and the length sit inside it, and its C size.
    let string_info = string.and_then(|s| {
        let size = c_layout::type_layout(&s.name, ir)?.size;
        let vec_field = s.fields.first()?;
        let vec_off = c_layout::field_offsets(&s.name, ir)?.first()?.0;
        let vec_struct = ir.find_struct(vec_field.type_name.trim())?;
        let vec_offs = c_layout::field_offsets(&vec_struct.name, ir)?;
        let at = |name: &str| {
            vec_struct
                .fields
                .iter()
                .position(|f| f.name == name)
                .map(|i| vec_offs[i].0 + vec_off)
        };
        Some((config.apply_prefix(&s.name), size, at("ptr")?, at("len")?))
    });

    builder.line("package Azul;");
    builder.line("use FFI::Platypus::Buffer ();");
    builder.blank();
    builder.line("# The address of a value's C bytes: a wrapper's record, a raw record, or");
    builder.line("# an address already. The buffer is unshared first, so C writes through");
    builder.line("# the address land in this value only.");
    builder.line("sub _addr {");
    builder.indent();
    builder.line("my ($v) = @_;");
    builder.line("$v = $$v if blessed($v) && $v->can('_az_wrapper');");
    builder.line("return $v unless ref $v;");
    builder.line("$$v .= '';");
    builder.line("my ($p) = FFI::Platypus::Buffer::scalar_to_buffer($$v);");
    builder.line("return $p;");
    builder.dedent();
    builder.line("}");
    builder.blank();
    builder.line("# A record argument: a wrapper hands over its record, anything else passes.");
    builder.line("sub _rec_arg {");
    builder.indent();
    builder.line("my ($v) = @_;");
    builder.line("return (blessed($v) && $v->can('_az_wrapper')) ? $$v : $v;");
    builder.dedent();
    builder.line("}");
    builder.blank();
    builder.line("# Mark a wrapper whose value was MOVED into C (or into a field): its DESTROY");
    builder.line("# must not free it again. Anything else is left alone.");
    builder.line("sub _consume {");
    builder.indent();
    builder.line("my ($v) = @_;");
    builder.line("$$v = undef if blessed($v) && $v->can('_az_wrapper');");
    builder.line("return;");
    builder.dedent();
    builder.line("}");
    builder.blank();
    builder.line("# A record object of FFI type `$alias` holding `$bytes` (C layout), padded");
    builder.line("# to the record's size.");
    builder.line("sub _rec {");
    builder.indent();
    builder.line("my ($alias, $bytes) = @_;");
    builder.line("my $n = $Azul::ffi->sizeof($alias);");
    builder.line("$bytes .= "\0" x ($n - length $bytes) if length($bytes) < $n;");
    builder.line("return bless \$bytes, "Azul::$alias";");
    builder.dedent();
    builder.line("}");
    builder.blank();
    builder.line("# The first `$size` C bytes of a wrapper or a record.");
    builder.line("sub _bytes_of {");
    builder.indent();
    builder.line("my ($v, $size) = @_;");
    builder.line("$v = $$v if blessed($v) && $v->can('_az_wrapper');");
    builder.line("die 'azul: expected an Azul value (moved out?)' unless ref $v && defined $$v;");
    builder.line("return substr($$v, 0, $size);");
    builder.dedent();
    builder.line("}");
    builder.blank();
    builder.line("# The bytes to MOVE into a field: a wrapper is consumed (its DESTROY no");
    builder.line("# longer frees them), a raw record is copied as it is.");
    builder.line("sub _take_bytes {");
    builder.indent();
    builder.line("my ($v, $size) = @_;");
    builder.line("my $b = _bytes_of($v, $size);");
    builder.line("_consume($v);");
    builder.line("return $b;");
    builder.dedent();
    builder.line("}");
    builder.blank();
    if let Some((alias, size, ptr_off, len_off)) = &string_info {
        builder.line("# The text of the AzString in `$bytes` (its C bytes), decoded from UTF-8.");
        builder.line("# The AzString is neither consumed nor freed.");
        builder.line("sub _read_string {");
        builder.indent();
        builder.line("my ($bytes) = @_;");
        builder.line(&format!("my $p = unpack('Q', substr($bytes, {}, 8));", ptr_off));
        builder.line(&format!("my $n = unpack('Q', substr($bytes, {}, 8));", len_off));
        builder.line("return '' unless $p && $n;");
        builder.line("my $t = FFI::Platypus::Buffer::buffer_to_scalar($p, $n);");
        builder.line("utf8::decode($t);");
        builder.line("return $t;");
        builder.dedent();
        builder.line("}");
        builder.blank();
        builder.line("# The C bytes of an AzString to MOVE into a field: a fresh copy of a Perl");
        builder.line("# string, or the bytes of a moved-in Azul::String wrapper / raw record.");
        builder.line("sub _string_bytes {");
        builder.indent();
        builder.line("my ($v) = @_;");
        builder.line(&format!("return _take_bytes($v, {}) if blessed($v);", size));
        builder.line("my $s = "$v";");
        builder.line("utf8::encode($s);");
        builder.line("my $n = length $s;");
        builder.line("my $buf = $n ? $s : "\0";");
        builder.line("my ($bp) = FFI::Platypus::Buffer::scalar_to_buffer($buf);");
        builder.line(&format!(
            "my $rec = Azul::FFI::{}_copyFromBytes($bp, 0, $n);",
            alias
        ));
        builder.line(&format!("return substr($$rec, 0, {});", size));
        builder.dedent();
        builder.line("}");
        builder.blank();
    }
}

/// Who a field accessor runs on: a wrapper (`$$self` is the record) or a
/// raw record (`$self` is the record).
#[derive(Clone, Copy, PartialEq)]
enum Receiver {
    Wrapper,
    Record,
}

/// `pack` / `unpack` letter of a C primitive (native byte order and size).
fn pack_letter(ty: &str) -> Option<&'static str> {
    Some(match ty {
        "bool" | "u8" | "c_uchar" => "C",
        "i8" | "c_char" | "char" => "c",
        "u16" => "S",
        "i16" => "s",
        "u32" | "c_uint" => "L",
        "i32" | "c_int" => "l",
        "u64" | "usize" => "Q",
        "i64" | "isize" => "q",
        "f32" | "c_float" => "f",
        "f64" | "c_double" => "d",
        _ => return None,
    })
}

/// `get_<field>` / `set_<field>` subs for the fields of `s` (the shared
/// contract in `field_access`), reading and writing the value's C bytes at
/// the offsets `c_layout` computes - not through the FFI::Platypus record
/// accessors, whose layouts flatten nested structs and unions.
///
/// - scalars `unpack` / `pack` in place (`bool` as 0 / 1, unit enums as
///   the C `int`);
/// - the String class reads as a Perl string (decoded, not consumed); its
///   setter takes a Perl string (copied into a fresh AzString) or moves an
///   `Azul::String` in, after releasing the old string (`AzString_delete`
///   on the field's address);
/// - a heap-owning struct reads as a deep copy (`Az<T>_clone`, wrapped so
///   DESTROY frees it) and is written by MOVING the new value in (the
///   wrapper is consumed) after `Az<T>_delete` released the old one;
/// - a POD struct / union reads as a fresh record copy and is written by
///   copying its bytes.
///
/// Getters return copies, so a nested write is read-modify-write:
///
/// ```perl
/// my $ws = $wco->get_window_state;      # a deep copy
/// $ws->set_title('Hello');              # the old title is released
/// my $size = $ws->get_size;
/// my $dims = $size->get_dimensions;
/// $dims->set_width(800); $dims->set_height(600);
/// $size->set_dimensions($dims); $ws->set_size($size);
/// $wco->set_window_state($ws);          # $ws is moved in
/// ```
///
/// A method of the same name (in `taken`) wins. Returns the number of subs.
fn emit_field_accessors(
    builder: &mut CodeBuilder,
    s: &StructDef,
    ir: &CodegenIR,
    config: &CodegenConfig,
    receiver: Receiver,
    taken: &BTreeSet<String>,
) -> usize {
    use super::super::{
        c_layout,
        field_access::{accessible_fields, FieldShape},
    };
    let Some(offsets) = c_layout::field_offsets(&s.name, ir) else {
        return 0;
    };
    let delete_set = collect_delete_targets(ir);
    let bytes = match receiver {
        Receiver::Wrapper => "my $b = $$self;",
        Receiver::Record => "my $b = $self;",
    };
    let mut n = 0;
    for (f, shape) in accessible_fields(s, ir, config) {
        let Some(i) = s.fields.iter().position(|x| std::ptr::eq(x, f)) else {
            continue;
        };
        let (off, layout) = offsets[i];
        let sz = layout.size;
        let field = format!("substr($$b, {}, {})", off, sz);
        let addr = format!("Azul::_addr($self) + {}", off);
        let (get, set): (Option<Vec<String>>, Option<Vec<String>>) = match &shape {
            FieldShape::Prim { ty, is_bool } => match pack_letter(ty) {
                Some(_) if *is_bool => (
                    Some(vec![format!("return unpack('C', {}) ? 1 : 0;", field)]),
                    Some(vec![format!("{} = pack('C', $v ? 1 : 0);", field)]),
                ),
                Some(l) => (
                    Some(vec![format!("return unpack('{}', {});", l, field)]),
                    Some(vec![format!("{} = pack('{}', $v);", field, l)]),
                ),
                None => (None, None),
            },
            FieldShape::UnitEnum { .. } if sz == 4 => (
                Some(vec![format!("return unpack('l', {});", field)]),
                Some(vec![format!("{} = pack('l', $v);", field)]),
            ),
            FieldShape::UnitEnum { .. } => (None, None),
            FieldShape::Str { delete, .. } => (
                Some(vec![format!("return Azul::_read_string({});", field)]),
                Some(vec![
                    "my $new = Azul::_string_bytes($v);".to_string(),
                    format!("Azul::FFI::{}({});", delete.c_name, addr),
                    format!("{} = $new;", field),
                ]),
            ),
            FieldShape::Value { name, delete: Some(d), clone } => {
                let wrapped = ir.find_struct(name).is_some_and(|t| should_emit_struct(t, config))
                    && delete_set.contains(name.as_str());
                let get = match clone {
                    Some(c) if wrapped => Some(vec![format!(
                        "return Azul::{}->new(Azul::FFI::{}({}));",
                        name, c.c_name, addr
                    )]),
                    // No owning wrapper class (a heap-owning union) or no
                    // deep copy: a copy would have no owner to free it.
                    _ => None,
                };
                (
                    get,
                    Some(vec![
                        format!("my $new = Azul::_take_bytes($v, {});", sz),
                        format!("Azul::FFI::{}({});", d.c_name, addr),
                        format!("{} = $new;", field),
                    ]),
                )
            }
            FieldShape::Value { name, delete: None, .. } => {
                let alias = config.apply_prefix(name);
                (
                    Some(vec![format!("return Azul::_rec('{}', {});", alias, field)]),
                    Some(vec![format!("{} = Azul::_bytes_of($v, {});", field, sz)]),
                )
            }
        };
        let getter = format!("get_{}", f.name);
        if let Some(lines) = get {
            if !taken.contains(&getter) {
                builder.line(&format!("# The `{}` field (a copy).", f.name));
                builder.line(&format!("sub {} {{", getter));
                builder.indent();
                builder.line("my $self = shift;");
                builder.line(bytes);
                for l in lines {
                    builder.line(&l);
                }
                builder.dedent();
                builder.line("}");
                builder.blank();
                n += 1;
            }
        }
        let setter = format!("set_{}", f.name);
        if let Some(lines) = set {
            if !taken.contains(&setter) {
                builder.line(&format!(
                    "# Replace the `{}` field (the old value is released); returns $self.",
                    f.name
                ));
                builder.line(&format!("sub {} {{", setter));
                builder.indent();
                builder.line("my ($self, $v) = @_;");
                builder.line(bytes);
                for l in lines {
                    builder.line(&l);
                }
                builder.line("return $self;");
                builder.dedent();
                builder.line("}");
                builder.blank();
                n += 1;
            }
        }
    }
    n
}

// ============================================================================
// Discovery
// ============================================================================

fn collect_delete_targets(ir: &CodegenIR) -> BTreeSet<&str> {
    ir.functions
        .iter()
        .filter(|f| f.kind == FunctionKind::Delete)
        .map(|f| f.class_name.as_str())
        .collect()
}

// ============================================================================
// Per-class emission
// ============================================================================

fn emit_class_wrapper(
    builder: &mut CodeBuilder,
    s: &StructDef,
    ir: &CodegenIR,
    config: &CodegenConfig,
) {
    let class_name = &s.name; // unprefixed, e.g. "App"
    let prefixed = config.apply_prefix(class_name); // e.g. "AzApp"
    let snake = snake_case(class_name); // e.g. "app"

    builder.line(&format!("package Azul::{} {{", class_name));
    builder.indent();
    builder.line("use strict;");
    builder.line("use warnings;");
    builder.blank();

    // Constructor: stores the pointer (or undef) in a blessed scalar ref.
    builder.line("sub new {");
    builder.indent();
    builder.line("my ($class, $ptr) = @_;");
    builder.line("my $self = \\$ptr;");
    builder.line("return bless $self, $class;");
    builder.dedent();
    builder.line("}");
    builder.blank();

    // Marks a wrapper (as opposed to a raw FFI::Platypus record, whose
    // field accessors may well include one named `ptr`).
    builder.line("sub _az_wrapper { 1 }");
    builder.blank();

    // Raw pointer accessor (escape hatch + used when one wrapper is passed
    // as an argument to another wrapper's method).
    builder.line("sub ptr {");
    builder.indent();
    builder.line("my $self = shift;");
    builder.line("return $$self;");
    builder.dedent();
    builder.line("}");
    builder.blank();

    // Destructor: Perl calls DESTROY exactly once, when the refcount hits 0.
    builder.line("sub DESTROY {");
    builder.indent();
    builder.line("my $self = shift;");
    builder.line("return unless defined $$self;");
    // `_delete` takes a pointer: the address of the record's bytes.
    builder.line(&format!(
        "Azul::FFI::{}_delete(Azul::_addr($self)) if Azul::FFI->can('{}_delete');",
        prefixed, prefixed
    ));
    builder.dedent();
    builder.line("}");
    builder.blank();

    // Methods.
    let mut emitted_any_method = false;
    let mut taken: BTreeSet<String> =
        ["new", "ptr", "DESTROY", "_az_wrapper"].iter().map(|n| n.to_string()).collect();
    for func in &ir.functions {
        if func.class_name != *class_name {
            continue;
        }
        if !should_emit_method(func) {
            continue;
        }
        emit_method(builder, func, &prefixed, &snake);
        taken.insert(perl_method_name(&func.method_name));
        emitted_any_method = true;
    }

    // Field accessors (after the methods: a method of the same name wins).
    if emit_field_accessors(builder, s, ir, config, Receiver::Wrapper, &taken) > 0 {
        emitted_any_method = true;
    }

    if !emitted_any_method {
        builder.line("# (no public methods exposed)");
    }

    builder.dedent();
    builder.line(&format!("}} # package Azul::{}", class_name));
}

/// Should this function be exposed as a Perl method on the wrapper?
///
/// We hide the auto-generated trait functions; `_delete` runs from
/// `DESTROY`, the others (`_partialEq`, `_hash`, ...) are surfaced via
/// custom Perl operators which we don't autogenerate today.
fn should_emit_method(func: &FunctionDef) -> bool {
    !matches!(
        func.kind,
        FunctionKind::Delete
            | FunctionKind::PartialEq
            | FunctionKind::PartialCmp
            | FunctionKind::Cmp
            | FunctionKind::Hash
            | FunctionKind::DebugToString
            | FunctionKind::EnumVariantConstructor
    )
}

fn emit_method(builder: &mut CodeBuilder, func: &FunctionDef, prefixed: &str, type_snake: &str) {
    let perl_method = perl_method_name(&func.method_name);
    let ffi_call = format!("Azul::FFI::{}", &func.c_name);

    let takes_self = matches!(func.kind, FunctionKind::Method | FunctionKind::MethodMut);

    let owning_class = prefixed.strip_prefix("Az").unwrap_or(prefixed).to_string();
    let returns_self_type = func
        .return_type
        .as_deref()
        .map(|t| t.trim() == owning_class)
        .unwrap_or(false);

    // Strip any explicit `self` (or class-named receiver) from the visible
    // arg list; Perl supplies it via `$$self`.
    let visible_args: Vec<&_> = func
        .args
        .iter()
        .filter(|a| a.name != "self" && a.name != type_snake)
        .collect();
    let arg_names: Vec<String> = visible_args
        .iter()
        .map(|a| perl_arg_name(&a.name))
        .collect();

    if takes_self {
        builder.line(&format!("sub {} {{", perl_method));
        builder.indent();
        let arg_decl = if arg_names.is_empty() {
            "my $self = shift;".to_string()
        } else {
            format!(
                "my ($self, {}) = @_;",
                arg_names
                    .iter()
                    .map(|n| format!("${}", n))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        builder.line(&arg_decl);
        let self_by_value = func
            .args
            .first()
            .map(|a| matches!(a.ref_kind, ArgRefKind::Owned))
            .unwrap_or(false);
        // A by-value receiver passes the record; a pointer receiver
        // (`&self` / `&mut self`) the ADDRESS of its bytes. Passing the
        // record object to an `opaque` parameter handed C the address of
        // the Perl SV.
        let mut call_args = vec![if self_by_value {
            "$$self".to_string()
        } else {
            "Azul::_addr($self)".to_string()
        }];
        for (n, a) in arg_names.iter().zip(visible_args.iter()) {
            call_args.push(arg_expr(n, a.ref_kind));
        }
        let call = format!("{}({})", ffi_call, call_args.join(", "));
        // Consume-after-by-value: if the C ABI took `$$self` by value
        // (args[0].ref_kind == Owned — DeepCopy / consuming-self),
        // Rust now owns those bytes. Set `$$self = undef;` so the
        // `DESTROY` magic method's `if defined $$self` guard
        // short-circuits on cleanup. Mirrors the Pascal/Fortran/JVM
        // `__consume` / FOwned/owned-flag pattern.
        let mut consume = if self_by_value { "$$self = undef;".to_string() } else { String::new() };
        for (n, a) in arg_names.iter().zip(visible_args.iter()) {
            if matches!(a.ref_kind, ArgRefKind::Owned) {
                consume.push_str(&format!(" Azul::_consume(${});", n));
            }
        }
        emit_method_body_with_consume(
            builder,
            &call,
            &func.return_type,
            returns_self_type,
            consume.trim(),
        );
        builder.dedent();
        builder.line("}");
        builder.blank();
        return;
    }

    // Static / class method.
    builder.line(&format!("sub {} {{", perl_method));
    builder.indent();
    let arg_decl = if arg_names.is_empty() {
        "my $class = shift;".to_string()
    } else {
        format!(
            "my ($class, {}) = @_;",
            arg_names
                .iter()
                .map(|n| format!("${}", n))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    builder.line(&arg_decl);
    let call_args: Vec<String> = arg_names
        .iter()
        .zip(visible_args.iter())
        .map(|(n, a)| arg_expr(n, a.ref_kind))
        .collect();
    let call = format!("{}({})", ffi_call, call_args.join(", "));
    let consume: Vec<String> = arg_names
        .iter()
        .zip(visible_args.iter())
        .filter(|(_, a)| matches!(a.ref_kind, ArgRefKind::Owned))
        .map(|(n, _)| format!("Azul::_consume(${});", n))
        .collect();
    emit_method_body_with_consume(
        builder,
        &call,
        &func.return_type,
        returns_self_type,
        &consume.join(" "),
    );
    builder.dedent();
    builder.line("}");
    builder.blank();
}

/// Body of a wrapper method.
///
/// If the C function returns the same struct as the wrapper class, wrap
/// the result in `$class->new($ptr)` so the caller gets a managed instance
/// with a finalizer. Otherwise return the raw FFI result verbatim.
///
/// `consume_stmt` runs between the call and the return: `$$self = undef;`
/// for a by-value receiver and `Azul::_consume($arg);` for every by-value
/// argument, so DESTROY never frees bytes the C ABI took over.
fn emit_method_body_with_consume(
    builder: &mut CodeBuilder,
    call: &str,
    return_type: &Option<String>,
    returns_self_type: bool,
    consume_stmt: &str,
) {
    let has_consume = !consume_stmt.is_empty();
    match return_type {
        None => {
            builder.line(&format!("{};", call));
            if has_consume {
                builder.line(consume_stmt);
            }
        }
        Some(_) if returns_self_type => {
            if has_consume {
                builder.line(&format!("my $_raw = {};", call));
                builder.line(consume_stmt);
                builder.line("return __PACKAGE__->new($_raw);");
            } else {
                builder.line(&format!("return __PACKAGE__->new({});", call));
            }
        }
        Some(_) => {
            if has_consume {
                builder.line(&format!("my $_ret = {};", call));
                builder.line(consume_stmt);
                builder.line("return $_ret;");
            } else {
                builder.line(&format!("return {};", call));
            }
        }
    }
}

/// One argument on the C call: a by-value argument passes a wrapper's
/// record (`Azul::_rec_arg`; the wrapper is consumed after the call), a
/// pointer argument the ADDRESS of the value's bytes (`Azul::_addr`).
/// Plain Perl values pass through either way.
fn arg_expr(name: &str, ref_kind: ArgRefKind) -> String {
    match ref_kind {
        ArgRefKind::Owned => format!("Azul::_rec_arg(${})", name),
        _ => format!("Azul::_addr(${})", name),
    }
}

// ============================================================================
// Naming helpers
// ============================================================================

/// Perl method names use snake_case. The IR's `method_name` is camelCase
/// (e.g. `addChild`) or already snake-ish — normalise either to snake.
fn perl_method_name(method: &str) -> String {
    let snake = camel_to_snake(method);
    // `new` collides with the bless-a-pointer constructor every
    // wrapper class already emits. Rename C-ABI factories named `new`
    // to `create` (same pattern Zig uses) so the redefinition warnings
    // go away and users see a single canonical constructor.
    if snake == "new" {
        return "create".to_string();
    }
    if PERL_RESERVED.contains(&snake.as_str()) {
        format!("{}_", snake)
    } else {
        snake
    }
}

fn perl_arg_name(name: &str) -> String {
    let snake = camel_to_snake(name);
    if PERL_RESERVED.contains(&snake.as_str()) {
        format!("{}_", snake)
    } else {
        snake
    }
}

fn camel_to_snake(input: &str) -> String {
    let mut out = String::with_capacity(input.len() + 4);
    let mut prev_was_lower = false;
    for (i, c) in input.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i != 0 && prev_was_lower {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
            prev_was_lower = false;
        } else {
            out.push(c);
            prev_was_lower = c.is_ascii_lowercase() || c.is_ascii_digit();
        }
    }
    out
}

const PERL_RESERVED: &[&str] = &[
    "and", "cmp", "continue", "do", "else", "elsif", "eq", "for", "foreach", "ge", "gt", "if",
    "le", "lt", "ne", "next", "no", "not", "or", "package", "redo", "require", "return", "sub",
    "unless", "until", "use", "while", "x", "xor",
];
