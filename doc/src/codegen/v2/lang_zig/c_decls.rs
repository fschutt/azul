//! Pre-translated C ABI for Zig: the body of `pub const C = struct { ... }` in `azul.zig`.
//!
//! `azul.zig` used to reach the C ABI through
//! `@cImport(@cInclude("azul.h"))`. That works, but the header is 5.6 MB
//! (types + ~20k `extern` declarations + ~100k lines of `static inline`
//! helpers), and Zig has to translate AND semantically analyse the
//! whole translation unit on every cold build: 91–123 s cold, ~5 s warm,
//! for a hello-world. This module emits the same surface directly from
//! the IR — the way every other binding gets its native declarations —
//! and `lang_zig::generate` embeds it, indented, as the `C` namespace of
//! the single generated file. Zig's lazy analysis then only touches the
//! declarations a program uses.
//!
//! # Fidelity contract
//!
//! The output is shaped after what `zig translate-c azul.h` produces for
//! `azul.h`, so code written against the `@cImport` namespace keeps
//! compiling unchanged (the example and the wrapper layer in
//! `wrappers.rs` are the regression tests):
//!
//! * structs → `extern struct` with zeroed defaults, unions →
//!   `extern union`, unit enums → a `c_uint` alias plus one `c_int`
//!   constant per variant (`AzUpdate_DoNothing`), exactly like the C enum
//!   `translate-c` sees;
//! * tagged unions → the `_Tag` constants, one `Variant_<V>` extern
//!   struct per variant (tag field `u8` for `#[repr(C, u8)]`, the tag
//!   enum type otherwise — the same rule as `lang_c.rs`), and the
//!   `extern union` over them;
//! * callback typedefs → `?*const fn (...) callconv(.c) R`;
//! * pointers → `[*c]T` / `[*c]const T` (`?*anyopaque` for `void*`), which
//!   coerce from `*T`, `?*T` and `null` the way C pointers do;
//! * functions → `pub extern fn <c_name>(...) R;` with the raw / `WithCtx`
//!   / `Struct` triplet for callback-taking API functions and the
//!   `Byref` twins, mirroring `lang_c.rs::generate_function_declaration`.
//!
//! What is deliberately NOT mirrored: the C header's `static inline`
//! helpers (`_isVariant`, `_matchRef*`, `AZ_REFLECT`, the Vec `_empty`
//! macros). They are C conveniences; Zig users have `switch`, the
//! `ReflectModel` prelude and the wrapper layer, and they are what made
//! `@cImport` slow in the first place.

use std::fmt::Write as _;

use super::{
    super::{
        config::CodegenConfig,
        ir::{
            ArgRefKind, CallbackTypedefDef, CodegenIR, EnumDef, EnumVariantKind, FieldRefKind,
            FunctionArg, FunctionDef, FunctionKind, MonomorphizedKind, MonomorphizedTypeDef,
            StructDef, TypeAliasDef,
        },
        managed_host_invoker::{has_callback_wrapper_arg, shadow_callback_typedef},
    },
    sanitize_identifier,
};

/// Generate the C declarations (types, constants, `pub extern fn`s), un-indented.
pub fn generate_c_decls(ir: &CodegenIR, config: &CodegenConfig) -> String {
    let mut out = String::with_capacity(4 << 20);

    out.push_str("// ---- Types ------------------------------------------------------------------\n\n");
    for s in &ir.structs {
        if !s.generic_params.is_empty() || !config.should_include_type(&s.name) {
            continue;
        }
        emit_struct(&mut out, s, config);
    }
    for e in &ir.enums {
        if !e.generic_params.is_empty() || !config.should_include_type(&e.name) {
            continue;
        }
        if e.is_union {
            emit_tagged_union(&mut out, e, config);
        } else {
            emit_unit_enum(&mut out, e, config);
        }
    }
    for cb in &ir.callback_typedefs {
        if !config.should_include_type(&cb.name) {
            continue;
        }
        emit_callback_typedef(&mut out, cb, config);
    }
    for ta in &ir.type_aliases {
        if !config.should_include_type(&ta.name) {
            continue;
        }
        emit_type_alias(&mut out, ta, config);
    }

    if !ir.constants.is_empty() {
        out.push_str("\n// ---- Constants --------------------------------------------------------------\n\n");
        for c in &ir.constants {
            let _ = writeln!(
                out,
                "pub const {} = {};",
                config.apply_prefix(&c.name),
                c.value.trim()
            );
        }
    }

    out.push_str("\n// ---- Functions --------------------------------------------------------------\n\n");
    for f in &ir.functions {
        if !config.should_include_type(&f.class_name) {
            continue;
        }
        emit_function(&mut out, f, config);
    }
    out
}

// ============================================================================
// Type mapping
// ============================================================================

/// Map an IR (Rust-spelled) type to its Zig spelling. Mirrors
/// `lang_c::rust_type_to_c_with_prefix` + what `translate-c` makes of the
/// resulting C type.
pub fn zig_type(rust_type: &str, config: &CodegenConfig) -> String {
    let t = rust_type.trim();
    if t.starts_with("ManuallyDrop<Box<") || t.starts_with("AzManuallyDrop<AzBox<") {
        return "?*anyopaque".to_string();
    }
    if let Some(inner) = t.strip_prefix("*const ") {
        return pointer_to(&zig_type(inner, config), true);
    }
    if let Some(inner) = t.strip_prefix("*mut ") {
        return pointer_to(&zig_type(inner, config), false);
    }
    if let Some(inner) = t.strip_prefix("&mut ") {
        return pointer_to(&zig_type(inner, config), false);
    }
    if let Some(inner) = t.strip_prefix('&') {
        return pointer_to(&zig_type(inner, config), true);
    }
    if let Some(p) = primitive(t) {
        return p.to_string();
    }
    config.apply_prefix(t)
}

/// C pointer to `inner`: `void*` becomes `?*anyopaque`, everything else
/// the `[*c]` C pointer `translate-c` emits (coerces from `*T`/`?*T`/null).
pub fn pointer_to(inner: &str, is_const: bool) -> String {
    let c = if is_const { "const " } else { "" };
    if inner == "anyopaque" || inner == "void" {
        format!("?*{}anyopaque", c)
    } else {
        format!("[*c]{}{}", c, inner)
    }
}

fn primitive(t: &str) -> Option<&'static str> {
    Some(match t {
        "bool" => "bool",
        "u8" | "c_uchar" | "c_char" => "u8",
        "u16" => "u16",
        "u32" => "u32",
        "u64" => "u64",
        "usize" => "usize",
        "i8" => "i8",
        "i16" => "i16",
        "i32" => "i32",
        "i64" => "i64",
        "isize" => "isize",
        "f32" | "c_float" => "f32",
        "f64" | "c_double" => "f64",
        // Only ever meaningful behind a pointer; `pointer_to` turns it into
        // `?*anyopaque`. As a bare return type `()` is `void`.
        "c_void" => "anyopaque",
        "()" | "void" => "void",
        "c_short" => "c_short",
        "c_ushort" => "c_ushort",
        "c_int" => "c_int",
        "c_uint" => "c_uint",
        "c_long" => "c_long",
        "c_ulong" => "c_ulong",
        "c_longlong" => "c_longlong",
        "c_ulonglong" => "c_ulonglong",
        _ => return None,
    })
}

/// `[T; N]` → (`T`, `Some(N)`).
fn split_array(t: &str) -> (String, Option<String>) {
    let t = t.trim();
    if t.starts_with('[') && t.ends_with(']') {
        let inner = &t[1..t.len() - 1];
        if let Some(pos) = inner.rfind(';') {
            let base = inner[..pos].trim();
            let n = inner[pos + 1..].trim();
            if n.parse::<usize>().is_ok() {
                return (base.to_string(), Some(n.to_string()));
            }
        }
    }
    (t.to_string(), None)
}

/// A struct field's full Zig type, honouring the ref kind and arrays.
fn field_type(type_name: &str, ref_kind: &FieldRefKind, config: &CodegenConfig) -> String {
    let (base, array) = split_array(type_name);
    let base_zig = zig_type(&base, config);
    let ty = match ref_kind {
        FieldRefKind::Owned => base_zig,
        FieldRefKind::Ref | FieldRefKind::Ptr => pointer_to(&base_zig, true),
        FieldRefKind::RefMut
        | FieldRefKind::PtrMut
        | FieldRefKind::Boxed
        | FieldRefKind::OptionBoxed => pointer_to(&base_zig, false),
    };
    match array {
        Some(n) => format!("[{}]{}", n, ty),
        None => ty,
    }
}

fn arg_type(type_name: &str, ref_kind: ArgRefKind, config: &CodegenConfig) -> String {
    let base = zig_type(type_name, config);
    match ref_kind {
        ArgRefKind::Owned => base,
        ArgRefKind::Ref | ArgRefKind::Ptr => pointer_to(&base, true),
        ArgRefKind::RefMut | ArgRefKind::PtrMut => pointer_to(&base, false),
    }
}

/// Zero default for an `extern struct` field, so `.{}` / partial struct
/// literals work like they did on the `translate-c` output.
fn zero_default(zig_ty: &str) -> String {
    match zig_ty {
        "bool" => "false".to_string(),
        "f32" | "f64" => "0".to_string(),
        "u8" | "u16" | "u32" | "u64" | "usize" | "i8" | "i16" | "i32" | "i64" | "isize"
        | "c_short" | "c_ushort" | "c_int" | "c_uint" | "c_long" | "c_ulong" | "c_longlong"
        | "c_ulonglong" => "0".to_string(),
        t if t.starts_with("[*c]") || t.starts_with("?*") => "null".to_string(),
        t => format!("std.mem.zeroes({})", t),
    }
}

fn field_line(out: &mut String, name: &str, zig_ty: &str) {
    let _ = writeln!(
        out,
        "    {}: {} = {},",
        sanitize_identifier(name),
        zig_ty,
        zero_default(zig_ty)
    );
}

// ============================================================================
// Types
// ============================================================================

fn emit_struct(out: &mut String, s: &StructDef, config: &CodegenConfig) {
    let name = config.apply_prefix(&s.name);
    let _ = writeln!(out, "pub const {} = extern struct {{", name);
    if s.fields.is_empty() {
        // Same dummy byte the C header uses for an empty struct.
        field_line(out, "_dummy", "u8");
    }
    for f in &s.fields {
        let ty = field_type(&f.type_name, &f.ref_kind, config);
        field_line(out, &f.name, &ty);
    }
    out.push_str("};\n");
}

fn emit_unit_enum(out: &mut String, e: &EnumDef, config: &CodegenConfig) {
    let name = config.apply_prefix(&e.name);
    let variants: Vec<&str> = e.variants.iter().map(|v| v.name.as_str()).collect();
    emit_c_enum(out, &name, &variants, is_u8_repr(e.repr.as_deref()), "_Force8Bit");
}

/// A C `enum`: a `c_uint` alias and one `c_int` constant per enumerator
/// (the `translate-c` shape). `sentinel` is the `= 0xFF` size-forcing
/// enumerator the C header adds for `#[repr(u8)]`.
fn emit_c_enum(out: &mut String, name: &str, variants: &[&str], u8_repr: bool, sentinel: &str) {
    for (i, v) in variants.iter().enumerate() {
        let _ = writeln!(out, "pub const {}_{}: c_int = {};", name, v, i);
    }
    if u8_repr {
        let _ = writeln!(out, "pub const {}{}: c_int = 255;", name, sentinel);
    }
    let _ = writeln!(out, "pub const {} = c_uint;", name);
}

fn is_u8_repr(repr: Option<&str>) -> bool {
    repr.map(|r| r.contains("u8")).unwrap_or(false)
}

fn emit_tagged_union(out: &mut String, e: &EnumDef, config: &CodegenConfig) {
    let name = config.apply_prefix(&e.name);
    let u8_repr = is_u8_repr(e.repr.as_deref());
    let tag_name = format!("{}_Tag", name);
    let variants: Vec<&str> = e.variants.iter().map(|v| v.name.as_str()).collect();
    emit_c_enum(out, &tag_name, &variants, u8_repr, "__Force8Bit");
    let tag_ty = if u8_repr { "u8".to_string() } else { tag_name.clone() };

    for v in &e.variants {
        let _ = writeln!(out, "pub const {}Variant_{} = extern struct {{", name, v.name);
        field_line(out, "tag", &tag_ty);
        match &v.kind {
            EnumVariantKind::Tuple(types) if !types.is_empty() => {
                for (i, (ty, rk)) in types.iter().enumerate() {
                    let zt = field_type(ty, rk, config);
                    if types.len() == 1 {
                        field_line(out, "payload", &zt);
                    } else {
                        field_line(out, &format!("payload_{}", i), &zt);
                    }
                }
            }
            EnumVariantKind::Struct(fields) if !fields.is_empty() => {
                for f in fields {
                    let zt = zig_type(&f.type_name, config);
                    field_line(out, &f.name, &zt);
                }
            }
            _ => {}
        }
        out.push_str("};\n");
    }
    emit_union(out, &name, &variants);
}

fn emit_union(out: &mut String, name: &str, variants: &[&str]) {
    if variants.is_empty() {
        // An empty C union has no Zig spelling; keep the name resolvable.
        let _ = writeln!(out, "pub const {} = extern struct {{ _dummy: u8 = 0 }};", name);
        return;
    }
    let _ = writeln!(out, "pub const {} = extern union {{", name);
    for v in variants {
        let _ = writeln!(
            out,
            "    {}: {}Variant_{},",
            sanitize_identifier(v),
            name,
            v
        );
    }
    out.push_str("};\n");
}

fn emit_callback_typedef(out: &mut String, cb: &CallbackTypedefDef, config: &CodegenConfig) {
    let name = config.apply_prefix(&cb.name);
    let args: Vec<String> = cb
        .args
        .iter()
        .map(|a| arg_type(&a.type_name, a.ref_kind, config))
        .collect();
    let ret = cb
        .return_type
        .as_deref()
        .map(|r| zig_type(r, config))
        .unwrap_or_else(|| "void".to_string());
    let _ = writeln!(
        out,
        "pub const {} = ?*const fn ({}) callconv(.c) {};",
        name,
        args.join(", "),
        ret
    );
}

fn emit_type_alias(out: &mut String, ta: &TypeAliasDef, config: &CodegenConfig) {
    let name = config.apply_prefix(&ta.name);
    if let Some(mono) = &ta.monomorphized_def {
        emit_monomorphized(out, &name, mono, config);
        return;
    }
    let target = ta.target.trim();
    if target.contains('<') || target.contains('>') {
        return;
    }
    let _ = writeln!(out, "pub const {} = {};", name, zig_type(target, config));
}

fn emit_monomorphized(
    out: &mut String,
    name: &str,
    mono: &MonomorphizedTypeDef,
    config: &CodegenConfig,
) {
    match &mono.kind {
        MonomorphizedKind::TaggedUnion { repr, variants } => {
            let u8_repr = is_u8_repr(repr.as_deref());
            let tag_name = format!("{}_Tag", name);
            let names: Vec<&str> = variants.iter().map(|v| v.name.as_str()).collect();
            emit_c_enum(out, &tag_name, &names, u8_repr, "__Force8Bit");
            let tag_ty = if u8_repr { "u8".to_string() } else { tag_name.clone() };
            for v in variants {
                let _ = writeln!(out, "pub const {}Variant_{} = extern struct {{", name, v.name);
                field_line(out, "tag", &tag_ty);
                if let Some(p) = &v.payload_type {
                    let zt = field_type(p, &v.payload_ref_kind, config);
                    field_line(out, "payload", &zt);
                }
                out.push_str("};\n");
            }
            emit_union(out, name, &names);
        }
        MonomorphizedKind::SimpleEnum { repr, variants } => {
            let names: Vec<&str> = variants.iter().map(|v| v.as_str()).collect();
            emit_c_enum(out, name, &names, is_u8_repr(repr.as_deref()), "_Force8Bit");
        }
        MonomorphizedKind::Struct { fields } => {
            let _ = writeln!(out, "pub const {} = extern struct {{", name);
            if fields.is_empty() {
                field_line(out, "_dummy", "u8");
            }
            for f in fields {
                let zt = field_type(&f.type_name, &f.ref_kind, config);
                field_line(out, &f.name, &zt);
            }
            out.push_str("};\n");
        }
    }
}

// ============================================================================
// Functions
// ============================================================================

fn param(name: &str, ty: &str) -> String {
    format!("{}: {}", sanitize_identifier(name), ty)
}

fn extern_fn(out: &mut String, c_name: &str, params: &[String], ret: &str) {
    let _ = writeln!(
        out,
        "pub extern fn {}({}) {};",
        c_name,
        params.join(", "),
        ret
    );
}

/// Mirror of `lang_c::generate_function_declaration`: the plain
/// declaration, or the raw / `WithCtx` / `Struct` triplet for API functions
/// taking a callback wrapper, each followed by its `Byref` twin.
fn emit_function(out: &mut String, f: &FunctionDef, config: &CodegenConfig) {
    let has_cb_wrapper_arg = has_callback_wrapper_arg(f);

    let ret = f
        .return_type
        .as_deref()
        .map(|r| zig_type(r, config))
        .unwrap_or_else(|| "void".to_string());

    if !has_cb_wrapper_arg {
        let params: Vec<String> = f
            .args
            .iter()
            .map(|a| param(&a.name, &arg_type(&a.type_name, a.ref_kind, config)))
            .collect();
        extern_fn(out, &f.c_name, &params, &ret);
        emit_byref_twin(out, &f.c_name, &ret, f.return_type.is_some(), &f.args, config);
        return;
    }

    let mut raw_args: Vec<FunctionArg> = Vec::with_capacity(f.args.len());
    let mut ctx_args: Vec<FunctionArg> = Vec::with_capacity(f.args.len() + 1);
    for a in &f.args {
        let cb_typedef = shadow_callback_typedef(f, a);
        let is_cb = cb_typedef.is_some();
        let mut r = a.clone();
        if let Some(td) = cb_typedef {
            r.type_name = td.to_string();
        }
        raw_args.push(r.clone());
        ctx_args.push(r);
        if is_cb {
            let mut c = a.clone();
            c.name = format!("{}_ctx", a.name);
            c.type_name = "OptionRefAny".to_string();
            c.ref_kind = ArgRefKind::Owned;
            ctx_args.push(c);
        }
    }
    let to_params = |args: &[FunctionArg]| -> Vec<String> {
        args.iter()
            .map(|a| param(&a.name, &arg_type(&a.type_name, a.ref_kind, config)))
            .collect()
    };
    let has_ret = f.return_type.is_some();
    out.push_str("// Raw variant: fn pointer only (ctx is implicitly None).\n");
    extern_fn(out, &f.c_name, &to_params(&raw_args), &ret);
    out.push_str("// WithCtx variant: fn pointer + AzOptionRefAny ctx for host-handle dispatch.\n");
    let with_ctx = format!("{}WithCtx", f.c_name);
    extern_fn(out, &with_ctx, &to_params(&ctx_args), &ret);
    out.push_str("// Struct variant: the whole callback-wrapper struct by value (cb + ctx).\n");
    let with_struct = format!("{}Struct", f.c_name);
    extern_fn(out, &with_struct, &to_params(&f.args), &ret);
    emit_byref_twin(out, &f.c_name, &ret, has_ret, &raw_args, config);
    emit_byref_twin(out, &with_ctx, &ret, has_ret, &ctx_args, config);
    emit_byref_twin(out, &with_struct, &ret, has_ret, &f.args, config);
}

/// `<c_name>Byref`: owned aggregates by pointer, return via out-pointer.
/// Emitted under exactly the predicate `lang_c::emit_c_byref_twin` uses,
/// so every twin declared here exists in the DLL.
fn emit_byref_twin(
    out: &mut String,
    c_name: &str,
    ret: &str,
    has_return: bool,
    args: &[FunctionArg],
    config: &CodegenConfig,
) {
    let is_aggregate = |a: &FunctionArg| {
        matches!(a.ref_kind, ArgRefKind::Owned)
            && a.type_name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
            && !a.type_name.ends_with("CallbackType")
            && !a.type_name.ends_with("FnType")
    };
    if !args.iter().any(is_aggregate) {
        return;
    }
    let mut params: Vec<String> = Vec::with_capacity(args.len() + 1);
    if has_return {
        params.push(format!("__ret: {}", pointer_to(ret, false)));
    }
    for a in args {
        let ty = if is_aggregate(a) {
            pointer_to(&zig_type(&a.type_name, config), false)
        } else {
            arg_type(&a.type_name, a.ref_kind, config)
        };
        params.push(param(&a.name, &ty));
    }
    let _ = writeln!(out, "pub extern fn {}Byref({}) void;", c_name, params.join(", "));
}

#[cfg(test)]
pub(crate) mod tests {
    use super::super::super::ir::{EnumVariantDef, FieldDef, FunctionKind, TypeCategory};
    use super::*;

    fn field(name: &str, ty: &str, rk: FieldRefKind) -> FieldDef {
        FieldDef {
            name: name.into(),
            type_name: ty.into(),
            doc: None,
            is_public: true,
            ref_kind: rk,
        }
    }

    fn arg(name: &str, ty: &str, rk: ArgRefKind) -> FunctionArg {
        FunctionArg {
            name: name.into(),
            type_name: ty.into(),
            ref_kind: rk,
            doc: None,
            callback_info: None,
        }
    }

    fn func(
        c_name: &str,
        class: &str,
        kind: FunctionKind,
        args: Vec<FunctionArg>,
        ret: Option<&str>,
    ) -> FunctionDef {
        FunctionDef {
            c_name: c_name.into(),
            class_name: class.into(),
            method_name: c_name.rsplit('_').next().unwrap().into(),
            kind,
            args,
            return_type: ret.map(str::to_string),
            fn_body: None,
            doc: vec![],
            is_const: false,
            is_unsafe: false,
        }
    }

    fn enum_def(name: &str, is_union: bool, repr: &str, variants: Vec<EnumVariantDef>) -> EnumDef {
        EnumDef {
            name: name.into(),
            doc: vec![],
            variants,
            external_path: None,
            module: "dom".into(),
            derives: vec![],
            has_explicit_derive: false,
            is_union,
            repr: Some(repr.into()),
            is_send_safe: true,
            traits: Default::default(),
            generic_params: vec![],
            category: TypeCategory::Regular,
            dependencies: vec![],
            sort_order: 0,
            needs_forward_decl: false,
        }
    }

    fn unit(name: &str) -> EnumVariantDef {
        EnumVariantDef {
            name: name.into(),
            doc: None,
            kind: EnumVariantKind::Unit,
        }
    }

    /// One struct (with a pointer, an array and a keyword-named field), a
    /// unit enum, a `#[repr(C, u8)]` tagged union, a callback typedef and
    /// three functions — every shape `azul.h` has, in miniature. Shared
    /// with the Kotlin generator's direct-mapping test.
    pub(crate) fn fixture_ir() -> CodegenIR {
        let mut ir = CodegenIR::new();
        ir.structs.push(StructDef {
            name: "Foo".into(),
            doc: vec![],
            fields: vec![
                field("count", "u32", FieldRefKind::Owned),
                field("next", "Foo", FieldRefKind::Ptr),
                field("name", "[u8; 4]", FieldRefKind::Owned),
                field("type", "Update", FieldRefKind::Owned),
                field("data", "*const c_void", FieldRefKind::Owned),
            ],
            external_path: None,
            module: "dom".into(),
            derives: vec![],
            has_explicit_derive: false,
            custom_impls: vec![],
            is_boxed: false,
            repr: Some("C".into()),
            is_send_safe: true,
            generic_params: vec![],
            traits: Default::default(),
            category: TypeCategory::Regular,
            dependencies: vec![],
            sort_order: 0,
            needs_forward_decl: false,
            callback_wrapper_info: None,
        });
        ir.enums.push(enum_def(
            "Update",
            false,
            "C",
            vec![unit("DoNothing"), unit("RefreshDom")],
        ));
        ir.enums.push(enum_def(
            "OptionFoo",
            true,
            "C, u8",
            vec![
                unit("None"),
                EnumVariantDef {
                    name: "Some".into(),
                    doc: None,
                    kind: EnumVariantKind::Tuple(vec![("Foo".into(), FieldRefKind::Owned)]),
                },
            ],
        ));
        ir.callback_typedefs.push(CallbackTypedefDef {
            name: "FooCallbackType".into(),
            args: vec![
                arg("foo", "Foo", ArgRefKind::Owned),
                arg("out", "Foo", ArgRefKind::RefMut),
            ],
            return_type: Some("Update".into()),
            doc: vec![],
            module: "dom".into(),
            external_path: None,
            wrapper: None,
            dependencies: vec![],
            sort_order: 0,
        });
        ir.functions.push(func(
            "AzFoo_create",
            "Foo",
            FunctionKind::Constructor,
            vec![
                arg("count", "u32", ArgRefKind::Owned),
                arg("mode", "Update", ArgRefKind::Owned),
            ],
            Some("Foo"),
        ));
        ir.functions.push(func(
            "AzFoo_bump",
            "Foo",
            FunctionKind::MethodMut,
            vec![
                arg("foo", "Foo", ArgRefKind::RefMut),
                arg("align", "usize", ArgRefKind::Owned),
            ],
            None,
        ));
        ir.functions.push(func(
            "AzFoo_delete",
            "Foo",
            FunctionKind::Delete,
            vec![arg("foo", "Foo", ArgRefKind::RefMut)],
            None,
        ));
        ir.type_to_module.insert("Foo".into(), "dom".into());
        ir
    }

    fn generate_fixture() -> String {
        generate_c_decls(&fixture_ir(), &CodegenConfig::c_header())
    }

    #[test]
    fn structs_are_extern_with_c_pointer_fields_arrays_and_escaped_names() {
        let z = generate_fixture();
        assert!(z.contains("pub const AzFoo = extern struct {\n"), "{z}");
        assert!(z.contains("    count: u32 = 0,\n"), "{z}");
        assert!(z.contains("    next: [*c]const AzFoo = null,\n"), "{z}");
        assert!(z.contains("    name: [4]u8 = std.mem.zeroes([4]u8),\n"), "{z}");
        assert!(
            z.contains("    @\"type\": AzUpdate = std.mem.zeroes(AzUpdate),\n"),
            "{z}"
        );
        assert!(z.contains("    data: ?*const anyopaque = null,\n"), "{z}");
    }

    #[test]
    fn unit_enums_keep_the_translate_c_shape() {
        let z = generate_fixture();
        assert!(z.contains("pub const AzUpdate_DoNothing: c_int = 0;\n"), "{z}");
        assert!(z.contains("pub const AzUpdate_RefreshDom: c_int = 1;\n"), "{z}");
        assert!(z.contains("pub const AzUpdate = c_uint;\n"), "{z}");
    }

    #[test]
    fn u8_tagged_unions_have_a_u8_tag_per_variant_struct_and_an_extern_union() {
        let z = generate_fixture();
        assert!(z.contains("pub const AzOptionFoo_Tag_Some: c_int = 1;\n"), "{z}");
        assert!(z.contains("pub const AzOptionFoo_Tag__Force8Bit: c_int = 255;\n"), "{z}");
        assert!(
            z.contains("pub const AzOptionFooVariant_None = extern struct {\n    tag: u8 = 0,\n};\n"),
            "{z}"
        );
        assert!(
            z.contains(
                "pub const AzOptionFooVariant_Some = extern struct {\n    tag: u8 = 0,\n    payload: AzFoo = std.mem.zeroes(AzFoo),\n};\n"
            ),
            "{z}"
        );
        assert!(
            z.contains(
                "pub const AzOptionFoo = extern union {\n    None: AzOptionFooVariant_None,\n    Some: AzOptionFooVariant_Some,\n};\n"
            ),
            "{z}"
        );
    }

    #[test]
    fn callback_typedefs_are_optional_c_fn_pointers() {
        let z = generate_fixture();
        assert!(
            z.contains(
                "pub const AzFooCallbackType = ?*const fn (AzFoo, [*c]AzFoo) callconv(.c) AzUpdate;\n"
            ),
            "{z}"
        );
    }

    #[test]
    fn functions_are_pub_extern_with_their_byref_twins() {
        let z = generate_fixture();
        assert!(
            z.contains("pub extern fn AzFoo_create(count: u32, mode: AzUpdate) AzFoo;\n"),
            "{z}"
        );
        // `Update` is an owned aggregate by lang_c's predicate, so the twin exists there too.
        assert!(
            z.contains(
                "pub extern fn AzFoo_createByref(__ret: [*c]AzFoo, count: u32, mode: [*c]AzUpdate) void;\n"
            ),
            "{z}"
        );
        assert!(
            z.contains("pub extern fn AzFoo_bump(foo: [*c]AzFoo, @\"align\": usize) void;\n"),
            "{z}"
        );
        assert!(!z.contains("AzFoo_bumpByref"), "no owned aggregate, no twin:\n{z}");
        assert!(z.contains("pub extern fn AzFoo_delete(foo: [*c]AzFoo) void;\n"), "{z}");
    }

    /// Count `{` minus `}` outside string literals and `//` comments, so a
    /// stray brace in emitted code (the kind `zig ast-check` rejects with
    /// "expected 'EOF', found '}'") fails here even without a Zig toolchain.
    fn brace_balance(src: &str) -> i64 {
        let mut depth = 0i64;
        for line in src.lines() {
            let bytes = line.as_bytes();
            let mut i = 0;
            let mut in_str = false;
            while i < bytes.len() {
                let c = bytes[i];
                if in_str {
                    if c == b'\\' {
                        i += 1;
                    } else if c == b'"' {
                        in_str = false;
                    }
                } else if c == b'"' {
                    in_str = true;
                } else if c == b'/' && bytes.get(i + 1) == Some(&b'/') {
                    break;
                } else if c == b'{' {
                    depth += 1;
                } else if c == b'}' {
                    depth -= 1;
                }
                i += 1;
            }
        }
        depth
    }

    #[test]
    fn azul_zig_embeds_the_pretranslation_instead_of_cimport() {
        let z = super::super::generate(&fixture_ir(), &CodegenConfig::c_header()).unwrap();
        assert!(z.contains("pub const C = struct {\n"), "{z}");
        // The declarations sit one level inside the namespace.
        assert!(z.contains("    pub const AzFoo = extern struct {\n"), "{z}");
        assert!(z.contains("    pub extern fn AzFoo_create("), "{z}");
        // Only the prose header may mention @cImport; no code line does.
        assert!(
            !z.lines().any(|l| !l.trim_start().starts_with("//") && l.contains("@cImport")),
            "{z}"
        );
        assert!(!z.contains("@cInclude"), "{z}");
        assert!(!z.contains("azul_c.zig"), "single-file binding:\n{z}");
        assert_eq!(brace_balance(&z), 0, "unbalanced braces in the emitted azul.zig");
        assert_eq!(brace_balance(&generate_c_decls(&fixture_ir(), &CodegenConfig::c_header())), 0);
        assert_eq!(brace_balance("const a = \"{\"; // }"), 0);
        assert_eq!(brace_balance("fn f() void {\n}}"), -1);
    }

    /// The real check, when a Zig toolchain is on PATH: the emitted file must parse.
    #[test]
    fn azul_zig_passes_zig_ast_check_when_zig_is_installed() {
        let probe = std::process::Command::new("zig").arg("version").output();
        if !probe.map(|o| o.status.success()).unwrap_or(false) {
            eprintln!("zig not found on PATH; skipping ast-check");
            return;
        }
        let z = super::super::generate(&fixture_ir(), &CodegenConfig::c_header()).unwrap();
        let path = std::env::temp_dir().join(format!("azul_zig_ast_check_{}.zig", std::process::id()));
        std::fs::write(&path, &z).unwrap();
        let out = std::process::Command::new("zig").arg("ast-check").arg(&path).output().unwrap();
        let _ = std::fs::remove_file(&path);
        assert!(
            out.status.success(),
            "zig ast-check failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
