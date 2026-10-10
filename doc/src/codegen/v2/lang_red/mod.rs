//! Red / Red/System (red-lang.org) binding generator.
//!
//! Emits a single Red/System include file, `azul.reds`, that talks to the
//! prebuilt `libazul` C-ABI shared library. Red is a *full-stack* language:
//! a high-level Rebol-like dialect (**Red**) on top of a low-level, C-like,
//! statically-typed dialect (**Red/System**). Only Red/System has a general
//! external-library FFI, so the binding is emitted in that dialect; high-level
//! Red consumes it through `routine!` / `#system-global` bridges (see the
//! guide). Both compile together with the same ~1 MB toolchain into a single
//! dependency-free native executable.
//!
//! FFI is feasible via Red/System, but this generator is ALPHA and
//! unverified: no Red toolchain is installed to compile-check its output.
//!
//! # Output structure
//!
//! ```red
//! Red/System [Title: "Azul bindings"]
//!
//! ;; Platform-specific library filename.
//! #either OS = 'Windows [ #define AZUL_LIB "azul.dll"    ] [
//! #either OS = 'macOS   [ #define AZUL_LIB "libazul.dylib" ] [
//!                         #define AZUL_LIB "libazul.so" ]]
//!
//! ;; --- Type aliases (C structs) ---
//! AzRefAny!: alias struct! [ ... ]
//! AzDom!:    alias struct! [ ... ]
//!
//! ;; --- Unit enums as integer #defines ---
//! #define AzButtonType_Primary 0
//!
//! ;; --- Imported C-ABI functions ---
//! #import [
//!     AZUL_LIB cdecl [
//!         AzApp_create: "AzApp_create" [
//!             data   [AzRefAny! value]
//!             config [AzAppConfig! value]
//!             return: [AzApp! value]
//!         ]
//!         ;; ...
//!     ]
//! ]
//!
//! ;; --- Host-invoker plumbing (callbacks + RefAny lifetime) ---
//! ;; per-kind [callback] dispatchers, register-<kind> helpers, releaser.
//! ```
//!
//! # Callbacks
//!
//! Red/System *can* produce direct C-callable function pointers (via the
//! `[callback]` function attribute and the `:fn` address-of operator), but the
//! binding routes callbacks through libazul's host-invoker plumbing anyway —
//! identically to the Fortran/Pascal bindings — because the per-kind invoker
//! signature is all-pointers + one out-pointer, keeping every aggregate
//! by-value plumbing on the well-trodden libazul C side rather than in the
//! least-exercised corner of Red/System's FFI.
//!
//! # Wiring
//!
//! Like other Wave-2 generators this module is intentionally NOT wired from
//! `v2/mod.rs`. The orchestrator would add `pub mod lang_red;` plus a
//! `pub fn generate_red(api_data) -> Result<String>` helper mirroring
//! `generate_fortran`, then write the result to `target/codegen/v2/azul.reds`.

use anyhow::Result;

use super::{
    c_layout::{self, type_layout},
    config::CodegenConfig,
    generator::CodeBuilder,
    ir::{
        ArgRefKind, CodegenIR, EnumDef, FieldDef, FunctionDef, MonomorphizedKind, StructDef,
        TypeAliasDef, TypeCategory,
    },
    managed_host_invoker::{has_return, host_invoker_kinds, managed_c_symbol, wrapper_name},
};

/// Base library name (without extension). Resolved per-platform to
/// `azul.dll` / `libazul.dylib` / `libazul.so` by the `#either OS` block.
pub const LIB_NAME: &str = "azul";

/// Fixed capacity for the host-handle / callback tables. Red/System has no
/// growable series in the low-level dialect the way high-level Red does, so
/// the binding pre-allocates parallel arrays. 4096 registered
/// callbacks/data-handles is far beyond any GUI's live-callback count.
pub const MAX_HANDLES: usize = 4096;

/// Public entry point. Produces the full `azul.reds` Red/System source.
pub fn generate(ir: &CodegenIR, config: &CodegenConfig) -> Result<String> {
    let mut b = CodeBuilder::new("    ");

    emit_header(&mut b);
    emit_library_directive(&mut b);
    emit_types(&mut b, ir, config);
    emit_imports(&mut b, ir, config);
    emit_host_invoker(&mut b, ir);

    Ok(b.finish())
}

fn emit_header(b: &mut CodeBuilder) {
    b.line("Red/System [");
    b.line("    Title:   \"Azul GUI framework bindings\"");
    b.line("    Author:  \"azul-doc codegen v2 (lang_red)\"");
    b.line("    Purpose: {Auto-generated Red/System FFI bindings for libazul.");
    b.line("              DO NOT EDIT MANUALLY.}");
    b.line("    Note:    {Red/System is the low-level dialect of Red. This file is");
    b.line("              #included by a Red/System program, or embedded into a");
    b.line("              high-level Red program via #system-global. See the guide.}");
    b.line("]");
    b.blank();
}

/// Resolve the platform-specific shared-library filename that `#import`
/// dlopen-loads at executable startup.
fn emit_library_directive(b: &mut CodeBuilder) {
    b.line(";; ------------------------------------------------------------------");
    b.line(";; Platform-specific shared-library filename for #import.");
    b.line(";; ------------------------------------------------------------------");
    b.line("#either OS = 'Windows [");
    b.line(&format!("    #define AZUL_LIB \"{}.dll\"", LIB_NAME));
    b.line("][ #either OS = 'macOS [");
    b.line(&format!("    #define AZUL_LIB \"lib{}.dylib\"", LIB_NAME));
    b.line("][");
    b.line(&format!("    #define AZUL_LIB \"lib{}.so\"", LIB_NAME));
    b.line("]]");
    b.blank();
}

// ============================================================================
// Types
// ============================================================================

/// Emit `alias struct!` type declarations (regular structs, field-accurate)
/// and unit-enum integer `#define`s. Tagged unions are emitted as flagged
/// opaque blobs (exact sizing is a follow-up).
fn emit_types(b: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    b.line(";; ------------------------------------------------------------------");
    b.line(";; Type aliases: C structs as `alias struct!`, unit enums as #define.");
    b.line(";; Emitted in dependency (sort_order) order so each alias is declared");
    b.line(";; before it is used as a by-value field/argument.");
    b.line(";; ------------------------------------------------------------------");
    b.blank();

    // Unit enums first — they lower to plain integer constants and never
    // depend on anything.
    for e in &ir.enums {
        if !config.should_include_type(&e.name) {
            continue;
        }
        if e.is_union || !e.generic_params.is_empty() {
            continue;
        }
        emit_unit_enum(b, e);
    }

    // Structs, tagged unions and the monomorphized aliases
    // (`LayoutWidthValue`, `OptionU32`, ...) together in dependency
    // (sort_order) order - azul.h's order - so every alias is declared
    // before a struct holds it by value. (All unions used to follow all
    // structs, and the monomorphized aliases were never declared.)
    enum Item<'a> {
        Struct(&'a StructDef),
        Union(&'a EnumDef),
        Alias(&'a TypeAliasDef),
    }
    let mut items: Vec<(usize, Item)> = Vec::new();
    for s in ir.structs.iter().filter(|s| should_emit_struct(s, config)) {
        items.push((s.sort_order, Item::Struct(s)));
    }
    for e in &ir.enums {
        if e.is_union && e.generic_params.is_empty() && config.should_include_type(&e.name) {
            items.push((e.sort_order, Item::Union(e)));
        }
    }
    for ta in &ir.type_aliases {
        if ta.monomorphized_def.is_some() && config.should_include_type(&ta.name) {
            items.push((ta.sort_order, Item::Alias(ta)));
        }
    }
    items.sort_by_key(|(order, _)| *order);
    for (_, item) in &items {
        match item {
            Item::Struct(s) => emit_struct_alias(b, s, ir),
            Item::Union(e) => emit_union_blob(b, &e.name, ir),
            Item::Alias(ta) => match ta.monomorphized_def.as_ref().map(|m| &m.kind) {
                Some(MonomorphizedKind::TaggedUnion { .. }) => emit_union_blob(b, &ta.name, ir),
                Some(MonomorphizedKind::Struct { fields }) => {
                    emit_fields_alias(b, &ta.name, fields, ir)
                }
                // A unit enum is an `integer!` (`map_owned_type`).
                Some(MonomorphizedKind::SimpleEnum { .. }) | None => {}
            },
        }
    }
    b.blank();
}

fn should_emit_struct(s: &StructDef, config: &CodegenConfig) -> bool {
    if !config.should_include_type(&s.name) {
        return false;
    }
    if !s.generic_params.is_empty() {
        return false;
    }
    !matches!(
        s.category,
        TypeCategory::Recursive | TypeCategory::VecRef | TypeCategory::GenericTemplate
    )
}

fn emit_unit_enum(b: &mut CodeBuilder, e: &EnumDef) {
    // Unit enums are C `enum`s (int-sized). Emit `#define AzFoo_Bar N`.
    let mut idx: i64 = 0;
    for (ref mut idx, v) in (0_i64..).zip(e.variants.iter()) {
        b.line(&format!("#define Az{}_{} {}", e.name, v.name, idx));
        *idx += 1;
    }
}

fn emit_struct_alias(b: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR) {
    emit_fields_alias(b, &s.name, &s.fields, ir);
}

/// `Az<name>!: alias struct! [..]` over plain C fields (a struct or a
/// monomorphized struct alias).
///
/// Every field sits at its azul.h offset (`c_layout::field_offsets`):
/// Red/System has no 16-bit or 64-bit integer and only a 32-bit `logic!`,
/// so a `bool` is one `byte!`, an `i16`/`u16` two `byte!`s (`name`,
/// `name_1`, little-endian), an `i64`/`u64` two `integer!`s (`name` = low
/// half, `name_hi`) - and since those are less aligned than the C type,
/// explicit `_pN [byte!]` fields fill the gaps C's alignment leaves and the
/// tail up to the C size. Every emitted field is then exactly as aligned as
/// Red/System wants it, so its own layout adds no padding.
fn emit_fields_alias(b: &mut CodeBuilder, name: &str, fields: &[FieldDef], ir: &CodegenIR) {
    b.line(&format!("Az{}!: alias struct! [", name));
    b.indent();
    if fields.is_empty() {
        // Red/System has no zero-field struct; azul.h gives an empty struct
        // a one-byte dummy member, so does this alias.
        b.line("_pad [byte!]        ;; placeholder (no public fields)");
    }
    let offsets = c_layout::field_offsets_of(fields, ir);
    let total = type_layout(name, ir).map(|l| l.size);
    let mut cur = 0usize;
    let mut pad = 0usize;
    let mut fill = |b: &mut CodeBuilder, cur: &mut usize, to: usize| {
        while *cur < to {
            b.line(&format!("_p{} [byte!]", pad));
            pad += 1;
            *cur += 1;
        }
    };
    for (i, f) in fields.iter().enumerate() {
        let tokens = field_tokens(&sanitize_ident(&f.name), &f.type_name, f.ref_kind, ir);
        if let Some(off) = offsets.as_ref().map(|o| o[i]) {
            fill(b, &mut cur, off);
            cur = off;
        }
        for (field, ty) in &tokens {
            b.line(&format!("{} [{}]", field, ty));
        }
        cur += member_size(&f.type_name, f.ref_kind, ir).unwrap_or(0);
    }
    if let (Some(total), Some(_)) = (total, offsets.as_ref()) {
        if !fields.is_empty() {
            fill(b, &mut cur, total);
        }
    }
    b.dedent();
    b.line("]");
    b.blank();
}

/// The C size of one field (a pointer for every non-owned ref kind).
fn member_size(
    type_name: &str,
    ref_kind: super::ir::FieldRefKind,
    ir: &CodegenIR,
) -> Option<usize> {
    match ref_kind {
        super::ir::FieldRefKind::Owned => type_layout(type_name, ir).map(|l| l.size),
        _ => Some(8),
    }
}

/// The C primitive a type name stands for, through simple type aliases
/// (`GLuint` -> `u32`).
fn primitive_of<'a>(t: &'a str, ir: &'a CodegenIR) -> &'a str {
    let mut t = t.trim();
    for _ in 0..8 {
        match ir.find_type_alias(t) {
            Some(ta) if ta.monomorphized_def.is_none() => t = ta.target.trim(),
            _ => break,
        }
    }
    t
}

/// The `name [type]` field(s) one C field becomes (see [`emit_fields_alias`]).
fn field_tokens(
    name: &str,
    type_name: &str,
    ref_kind: super::ir::FieldRefKind,
    ir: &CodegenIR,
) -> Vec<(String, String)> {
    let one = |ty: &str| vec![(name.to_string(), ty.to_string())];
    if ref_kind != super::ir::FieldRefKind::Owned {
        return one("byte-ptr!");
    }
    match primitive_of(type_name, ir) {
        "bool" | "GLboolean" => one("byte!"),
        "i16" | "u16" => vec![
            (name.to_string(), "byte!".to_string()),
            (format!("{}_1", name), "byte!".to_string()),
        ],
        "i64" | "u64" | "GLint64" | "GLuint64" => vec![
            (name.to_string(), "integer!".to_string()),
            (format!("{}_hi", name), "integer!".to_string()),
        ],
        _ => one(&field_type_token(type_name, ref_kind, ir)),
    }
}

/// A tagged union (a data-carrying enum or a monomorphized alias) as an
/// opaque blob of EXACTLY its C size and alignment, from the shared
/// `c_layout::type_layout` - the numbers azul.h, the Fortran blobs and the
/// host-invoker writeback use. It used to be one `byte-ptr!` (8 bytes)
/// whatever the union, so every by-value union argument, return and field
/// was the wrong size. The cells are the union's alignment wide
/// (`byte-ptr!` for 8, `integer!` for 4, else `byte!`), so the blob is
/// aligned like the C union and classified as integer words when it
/// travels in registers. Read the tag and payload through libazul's own
/// functions; Red/System has no view of the variants.
fn emit_union_blob(b: &mut CodeBuilder, name: &str, ir: &CodegenIR) {
    let Some(layout) = type_layout(name, ir) else {
        b.line(&format!(";; SKIPPED: Az{}! (no C layout)", name));
        b.blank();
        return;
    };
    let (cell, width) = match layout.align {
        a if a >= 8 => ("byte-ptr!", 8),
        4 => ("integer!", 4),
        _ => ("byte!", 1),
    };
    b.line(&format!(
        "Az{}!: alias struct! [    ;; tagged union: {} bytes, {}-aligned",
        name, layout.size, layout.align
    ));
    for i in 0..(layout.size / width).max(1) {
        b.line(&format!("    _{} [{}]", i, cell));
    }
    b.line("]");
    b.blank();
}

/// Red/System type token for a struct *field* of the given ref kind.
fn field_type_token(type_name: &str, ref_kind: super::ir::FieldRefKind, ir: &CodegenIR) -> String {
    use super::ir::FieldRefKind;
    match ref_kind {
        FieldRefKind::Ref
        | FieldRefKind::RefMut
        | FieldRefKind::Ptr
        | FieldRefKind::PtrMut
        | FieldRefKind::Boxed
        | FieldRefKind::OptionBoxed => "byte-ptr!".to_string(),
        FieldRefKind::Owned => map_owned_type(type_name, ir, /* as_field= */ true),
    }
}

/// Map an owned (by-value) Rust/IR type to its Red/System spelling.
///
/// `as_field = true` returns the token used inside a `struct!` field spec;
/// `false` returns the token used inside an `#import` argument spec. They
/// differ only for aggregates: a by-value struct field is `AzFoo! value`
/// while a by-value struct argument is also `AzFoo! value` — currently the
/// same, but kept separate so the field/arg conventions can diverge if a
/// future Red/System version needs it.
fn map_owned_type(rust_type: &str, ir: &CodegenIR, _as_field: bool) -> String {
    let t = rust_type.trim();

    // Raw pointers / references embedded in a type string.
    if t.starts_with("*const ") || t.starts_with("*mut ") || t.starts_with('&') {
        return "byte-ptr!".to_string();
    }

    match t {
        "bool" | "GLboolean" => "logic!".to_string(),
        "i8" | "u8" | "c_char" | "char" | "c_uchar" => "byte!".to_string(),
        // Red/System integer! is 32-bit; i16/u16/i32/u32 fit.
        "i16" | "u16" | "i32" | "u32" | "c_int" | "c_uint" | "GLint" | "GLuint" | "GLenum"
        | "GLbitfield" | "GLsizei" => "integer!".to_string(),
        // 64-bit ints: Red/System's integer! is 32-bit and it lacks a
        // portable int64. Represent as pointer-width so the ABI slot size is
        // right on LP64; VALUE access needs an int64 shim (see FINDINGS).
        "i64" | "u64" | "GLint64" | "GLuint64" => "byte-ptr!".to_string(),
        "f32" | "GLfloat" | "GLclampf" => "float32!".to_string(),
        "f64" | "GLdouble" | "GLclampd" => "float!".to_string(),
        "usize" | "size_t" | "uintptr_t" | "isize" | "ssize_t" | "intptr_t" | "GLsizeiptr"
        | "GLintptr" => "byte-ptr!".to_string(),
        "void" | "c_void" | "()" => "byte-ptr!".to_string(),
        _ => {
            // Known IR type → aliased struct passed by value, or an enum.
            if let Some(e) = ir.find_enum(t) {
                if e.is_union && e.generic_params.is_empty() {
                    // Opaque union blob, by value.
                    format!("Az{}! value", t)
                } else if e.generic_params.is_empty() {
                    // Unit enum → C int.
                    "integer!".to_string()
                } else {
                    "byte-ptr!".to_string()
                }
            } else if ir.callback_typedefs.iter().any(|c| c.name == t) {
                // Raw fn-ptr typedef.
                "byte-ptr!".to_string()
            } else if let Some(ta) = ir.find_type_alias(t) {
                match ta.monomorphized_def.as_ref().map(|m| &m.kind) {
                    // A monomorphized unit enum is a C enum: an int.
                    Some(MonomorphizedKind::SimpleEnum { .. }) => "integer!".to_string(),
                    Some(_) => format!("Az{}! value", t),
                    None => map_owned_type(&ta.target, ir, _as_field),
                }
            } else if ir.find_struct(t).is_some() {
                format!("Az{}! value", t)
            } else {
                "byte-ptr!".to_string()
            }
        }
    }
}

// ============================================================================
// Imported C-ABI functions
// ============================================================================

fn emit_imports(b: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    b.line(";; ------------------------------------------------------------------");
    b.line(";; Imported C-ABI functions. Symbol names match azul.h verbatim.");
    b.line(";; cdecl = the extern \"C\" convention libazul exports use.");
    b.line(";; ------------------------------------------------------------------");
    b.line("#import [");
    b.indent();
    b.line("AZUL_LIB cdecl [");
    b.indent();

    for func in &ir.functions {
        if !should_emit_function(func, ir, config) {
            continue;
        }
        emit_import(b, func, ir);
    }

    // Host-invoker C-ABI setters/getters that libazul exports.
    emit_host_invoker_imports(b, ir);

    b.dedent();
    b.line("]");
    b.dedent();
    b.line("]");
    b.blank();
}

fn should_emit_function(func: &FunctionDef, ir: &CodegenIR, config: &CodegenConfig) -> bool {
    // A trait entry point an api.json `derive` declares is not what the
    // `DestructorOrClone` exclusion below is for. That category is excluded
    // because those types' ordinary methods traffic in callback function
    // pointers this binding cannot marshal; `Az{T}_toDbgString(ptr) ->
    // AzString` traffics in neither, and is the same shape as the ~2800
    // `_toDbgString` declarations this binding already emits. Excluding it
    // wholesale is why every `*VecDestructor` declared `Debug` and named it
    // nowhere. (`*VecDestructor` is a tagged union, hence `find_enum`.)
    // RECURSIVE types are here for the same reason. `XmlNodeChild` and friends
    // are excluded below because their ORDINARY methods traffic in a shape
    // this binding cannot express by value - but `Az{T}_partialEq(a, b) ->
    // bool` and `Az{T}_toDbgString(ptr) -> AzString` take a pointer and return
    // a scalar, so the exclusion never applied to them. That is why the same
    // four types - `Xml`, `XmlNodeChild`, `XmlNodeChildVec`,
    // `ResultXmlXmlError` - showed up as the residue in fourteen bindings at
    // once: one cause, not fourteen.
    if func.kind.is_declared_capability()
        && (ir.find_enum(&func.class_name).is_some_and(|e| {
            matches!(
                e.category,
                TypeCategory::DestructorOrClone | TypeCategory::Recursive
            )
        }) || ir
            .find_struct(&func.class_name)
            .is_some_and(|s| s.category == TypeCategory::Recursive))
    {
        return config.should_include_type(&func.class_name);
    }

    if !config.should_include_type(&func.class_name) {
        return false;
    }
    if let Some(s) = ir.find_struct(&func.class_name) {
        if matches!(
            s.category,
            TypeCategory::Recursive
                | TypeCategory::VecRef
                | TypeCategory::DestructorOrClone
                | TypeCategory::GenericTemplate
        ) || !s.generic_params.is_empty()
        {
            return false;
        }
    }
    if let Some(e) = ir.find_enum(&func.class_name) {
        if matches!(
            e.category,
            TypeCategory::Recursive
                | TypeCategory::DestructorOrClone
                | TypeCategory::GenericTemplate
        ) || !e.generic_params.is_empty()
        {
            return false;
        }
    }
    true
}

fn emit_import(b: &mut CodeBuilder, func: &FunctionDef, ir: &CodegenIR) {
    // Functions with a callback-wrapper arg bind the `<c_name>Struct` C
    // symbol (whole wrapper struct by value) — same rule the C/Fortran/Pascal
    // bindings use; binding the bare fn-ptr symbol with a struct arg crashes.
    let c_symbol = managed_c_symbol(func);
    let red_name = reds_fn_name(&func.c_name);

    b.line(&format!("{}: \"{}\" [", red_name, c_symbol));
    b.indent();
    for arg in &func.args {
        let ty = match arg.ref_kind {
            ArgRefKind::Owned => map_owned_type(&arg.type_name, ir, false),
            ArgRefKind::Ref | ArgRefKind::RefMut | ArgRefKind::Ptr | ArgRefKind::PtrMut => {
                "byte-ptr!".to_string()
            }
        };
        b.line(&format!("{} [{}]", sanitize_ident(&arg.name), ty));
    }
    if let Some(ret) = &func.return_type {
        let ret_ty = map_owned_type(ret, ir, false);
        b.line(&format!("return: [{}]", ret_ty));
    }
    b.dedent();
    b.line("]");
}

/// The host-invoker C-ABI functions libazul exports, declared inside the same
/// `#import` block.
fn emit_host_invoker_imports(b: &mut CodeBuilder, ir: &CodegenIR) {
    b.blank();
    b.line(";; --- Host-invoker C-ABI (core/src/host_invoker.rs) ---");
    b.line("AzApp_setHostHandleReleaser: \"AzApp_setHostHandleReleaser\" [");
    b.line("    releaser [byte-ptr!]");
    b.line("]");
    b.line("AzRefAny_newHostHandle: \"AzRefAny_newHostHandle\" [");
    b.line("    id [byte-ptr!]              ;; u64 handle id (pointer-width slot)");
    b.line("    return: [AzRefAny! value]");
    b.line("]");
    b.line("AzRefAny_getHostHandle: \"AzRefAny_getHostHandle\" [");
    b.line("    refany [byte-ptr!]");
    b.line("    return: [byte-ptr!]        ;; u64 handle id (pointer-width slot)");
    b.line("]");
    for cb in host_invoker_kinds(ir) {
        let w = wrapper_name(cb);
        b.line(&format!(
            "AzApp_set{w}Invoker: \"AzApp_set{w}Invoker\" [",
            w = w
        ));
        b.line("    invoker [byte-ptr!]");
        b.line("]");
        b.line(&format!(
            "Az{w}_createFromHostHandle: \"Az{w}_createFromHostHandle\" [",
            w = w
        ));
        b.line("    id [byte-ptr!]");
        b.line(&format!("    return: [Az{w}! value]", w = w));
        b.line("]");
    }
}

// ============================================================================
// Host-invoker plumbing (Red/System side)
// ============================================================================

/// Per-kind `[callback]`-attributed dispatchers + register helpers + a shared
/// releaser + RefAny create/get + init. Mirrors the Fortran `managed.rs`
/// runtime, but with fixed-capacity parallel arrays (Red/System has no
/// growable low-level series).
fn emit_host_invoker(b: &mut CodeBuilder, ir: &CodegenIR) {
    b.line(";; ------------------------------------------------------------------");
    b.line(";; Host-invoker runtime (callbacks + RefAny lifetime).");
    b.line(";; Two parallel fixed-capacity tables share one id counter so the");
    b.line(";; releaser id space is unambiguous:");
    b.line(";;   - azul-handle-ids / azul-handle-ptrs : RefAny user-data pointers");
    b.line(";;   - azul-cb-ids     / azul-cb-fps      : registered callback fn-ptrs");
    b.line(";; ------------------------------------------------------------------");
    b.blank();

    b.line(&format!("#define AZUL_MAX_HANDLES {}", MAX_HANDLES));
    b.blank();
    b.line("azul-handle-ids:  as int-ptr!  0");
    b.line("azul-handle-ptrs: as int-ptr!  0    ;; array of pointer-width slots");
    b.line("azul-cb-ids:      as int-ptr!  0");
    b.line("azul-cb-fps:      as int-ptr!  0    ;; array of pointer-width slots");
    b.line("azul-next-id:     0");
    b.blank();

    // Allocate the tables. Called from azul-host-invoker-init.
    b.line("azul-tables-init: func [][");
    b.indent();
    b.line("azul-handle-ids:  as int-ptr! allocate AZUL_MAX_HANDLES * 4");
    b.line("azul-handle-ptrs: as int-ptr! allocate AZUL_MAX_HANDLES * size? byte-ptr!");
    b.line("azul-cb-ids:      as int-ptr! allocate AZUL_MAX_HANDLES * 4");
    b.line("azul-cb-fps:      as int-ptr! allocate AZUL_MAX_HANDLES * size? byte-ptr!");
    b.dedent();
    b.line("]");
    b.blank();

    // alloc-handle: store a pointer-width value, return the new id.
    b.line("azul-alloc-handle: func [value [byte-ptr!] return: [integer!]");
    b.line("    /local id [integer!] slot [int-ptr!]");
    b.line("][");
    b.indent();
    b.line("azul-next-id: azul-next-id + 1");
    b.line("id: azul-next-id");
    b.line("(azul-handle-ids + id)/value: id");
    b.line("slot: as int-ptr! (azul-handle-ptrs + id)");
    b.line("slot/value: as-integer value    ;; store the pointer bits");
    b.line("id");
    b.dedent();
    b.line("]");
    b.blank();

    b.line("azul-lookup-handle: func [id [integer!] return: [byte-ptr!]");
    b.line("    /local slot [int-ptr!]");
    b.line("][");
    b.indent();
    b.line("if id = 0 [return null]");
    b.line("slot: as int-ptr! (azul-handle-ptrs + id)");
    b.line("as byte-ptr! slot/value");
    b.dedent();
    b.line("]");
    b.blank();

    b.line("azul-alloc-cb: func [fp [byte-ptr!] return: [integer!]");
    b.line("    /local id [integer!] slot [int-ptr!]");
    b.line("][");
    b.indent();
    b.line("azul-next-id: azul-next-id + 1");
    b.line("id: azul-next-id");
    b.line("(azul-cb-ids + id)/value: id");
    b.line("slot: as int-ptr! (azul-cb-fps + id)");
    b.line("slot/value: as-integer fp");
    b.line("id");
    b.dedent();
    b.line("]");
    b.blank();

    b.line("azul-lookup-cb: func [id [integer!] return: [byte-ptr!]");
    b.line("    /local slot [int-ptr!]");
    b.line("][");
    b.indent();
    b.line("if id = 0 [return null]");
    b.line("slot: as int-ptr! (azul-cb-fps + id)");
    b.line("as byte-ptr! slot/value");
    b.dedent();
    b.line("]");
    b.blank();

    // Releaser: C-ABI callback fired by libazul on RefAny last-clone drop.
    b.line(";; Releaser — libazul calls this (u64 id) on last-clone drop.");
    b.line(";; The [cdecl] attribute gives it the C calling convention so it");
    b.line(";; can be handed to AzApp_setHostHandleReleaser as a fn-ptr.");
    b.line("azul-releaser: func [[cdecl] id [byte-ptr!]][");
    b.indent();
    b.line(";; Handles are never freed in the demo lifetime; a full binding");
    b.line(";; would clear (azul-handle-ptrs + id) / (azul-cb-fps + id) here.");
    b.line("id: id    ;; suppress unused-arg warning");
    b.dedent();
    b.line("]");
    b.blank();

    // RefAny create/get (high-level convenience).
    b.line("azul-refany-create: func [value [byte-ptr!] return: [AzRefAny! value]");
    b.line("    /local id [integer!]");
    b.line("][");
    b.indent();
    b.line("id: azul-alloc-handle value");
    b.line("AzRefAny_newHostHandle as byte-ptr! id");
    b.dedent();
    b.line("]");
    b.blank();

    b.line("azul-refany-get: func [refany [byte-ptr!] return: [byte-ptr!]");
    b.line("    /local id [integer!]");
    b.line("][");
    b.indent();
    b.line("id: as-integer AzRefAny_getHostHandle refany");
    b.line("azul-lookup-handle id");
    b.dedent();
    b.line("]");
    b.blank();

    // Per-kind invoker dispatchers + register helpers.
    for cb in host_invoker_kinds(ir) {
        emit_kind_dispatcher(b, cb);
    }

    // Init: allocate tables, register releaser + per-kind invokers.
    b.line("azul-host-invoker-init: func [][");
    b.indent();
    b.line("azul-tables-init");
    b.line("AzApp_setHostHandleReleaser as byte-ptr! :azul-releaser");
    for cb in host_invoker_kinds(ir) {
        let w = wrapper_name(cb);
        let snake = to_kebab(w);
        b.line(&format!(
            "AzApp_set{w}Invoker as byte-ptr! :azul-{s}-invoker",
            w = w,
            s = snake
        ));
    }
    b.dedent();
    b.line("]");
    b.blank();
}

/// Emit one kind's `[callback]` invoker dispatcher (the fn libazul calls,
/// pointer args only) and its `azul-register-<kind>` helper.
fn emit_kind_dispatcher(b: &mut CodeBuilder, cb: &super::ir::CallbackTypedefDef) {
    let w = wrapper_name(cb);
    let snake = to_kebab(w);

    // Arg list: id + one pointer per callback arg + out-ptr when non-void.
    let mut args: Vec<String> = vec!["id".to_string()];
    for i in 0..cb.args.len() {
        args.push(format!("arg{}", i));
    }
    if has_return(cb) {
        args.push("out".to_string());
    }
    // The user routine receives everything except `id`.
    let user_args: Vec<String> = args.iter().skip(1).cloned().collect();

    b.line(&format!(";; --- {} dispatcher ---", w));
    b.line(&format!(
        "azul-{s}-user!: alias function! [[cdecl] {u}]",
        s = snake,
        u = user_args
            .iter()
            .map(|a| format!("{} [byte-ptr!]", a))
            .collect::<Vec<_>>()
            .join(" ")
    ));
    b.blank();

    // The invoker libazul calls. [callback] because libazul stores & later
    // calls it. Signature: (u64 id, ptr args..., out ptr).
    b.line(&format!(
        "azul-{s}-invoker: func [[cdecl] {sig}",
        s = snake,
        sig = args
            .iter()
            .map(|a| format!("{} [byte-ptr!]", a))
            .collect::<Vec<_>>()
            .join(" ")
    ));
    b.line(&format!(
        "    /local fp [azul-{s}-user!] iid [integer!]",
        s = snake
    ));
    b.line("][");
    b.indent();
    b.line("iid: as-integer id");
    b.line(&format!(
        "fp: as azul-{s}-user! azul-lookup-cb iid",
        s = snake
    ));
    b.line("if null? as byte-ptr! fp [exit]");
    b.line(&format!("fp {}", user_args.join(" ")));
    b.dedent();
    b.line("]");
    b.blank();

    // Register helper: stash the user routine, mint the Az<Kind> value.
    b.line(&format!(
        "azul-register-{s}: func [cb [azul-{s}-user!] return: [Az{w}! value]",
        s = snake,
        w = w
    ));
    b.line("    /local id [integer!]");
    b.line("][");
    b.indent();
    b.line("id: azul-alloc-cb as byte-ptr! cb");
    b.line(&format!(
        "Az{w}_createFromHostHandle as byte-ptr! id",
        w = w
    ));
    b.dedent();
    b.line("]");
    b.blank();
}

// ============================================================================
// Name helpers
// ============================================================================

/// Red/System identifier for a C-ABI symbol. Red identifiers are permissive
/// (allow `-`, `!`, `?`); we keep the verbatim `AzFoo_bar` shape because the
/// `#import` word is bound to that name and used as the call site.
fn reds_fn_name(c_symbol: &str) -> String {
    sanitize_ident(c_symbol)
}

/// Sanitize an api.json field/arg name into a Red/System word. Red/System
/// reserves a handful of dialect words that would break an arg spec.
fn sanitize_ident(name: &str) -> String {
    let n = name.trim();
    if is_reds_reserved(n) {
        format!("{}_", n)
    } else if n.is_empty() {
        "arg_".to_string()
    } else {
        n.to_string()
    }
}

fn is_reds_reserved(name: &str) -> bool {
    matches!(
        name,
        "value"
            | "return"
            | "type"
            | "struct"
            | "alias"
            | "func"
            | "function"
            | "if"
            | "either"
            | "case"
            | "switch"
            | "while"
            | "until"
            | "any"
            | "all"
            | "as"
            | "null"
            | "true"
            | "false"
            | "none"
            | "exit"
            | "size?"
            | "declare"
            | "cdecl"
            | "stdcall"
            | "callback"
            | "local"
            | "integer"
            | "byte"
            | "logic"
            | "float"
            | "pointer"
            | "print"
    )
}

/// CamelCase → kebab-case (`ButtonOnClickCallback` → `button-on-click-callback`).
/// Red idiom uses kebab-case words.
fn to_kebab(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i != 0 {
                out.push('-');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::{bug_classes::ir, c_layout::type_layout, config::CodegenConfig};

    fn reds() -> &'static str {
        static OUT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
        OUT.get_or_init(|| super::generate(ir(), &CodegenConfig::c_header()).unwrap())
    }

    /// The field lines of `Az<name>!: alias struct! [ ... ]`.
    fn alias_fields(name: &str) -> Vec<&'static str> {
        let head = format!("Az{}!: alias struct! [\n", name);
        let out = reds();
        let start = out.find(&head).unwrap_or_else(|| panic!("no {head}")) + head.len();
        out[start..]
            .lines()
            .take_while(|l| l.trim() != "]")
            .map(str::trim)
            .collect()
    }

    /// Bytes one emitted field token occupies (Red/System on LP64).
    fn token_size(line: &str) -> usize {
        let tok = line
            .split_once('[')
            .and_then(|(_, r)| r.split_once(']'))
            .map(|(t, _)| t.trim())
            .unwrap_or_else(|| panic!("no type in `{line}`"));
        match tok {
            "byte!" => 1,
            "integer!" | "float32!" | "logic!" => 4,
            "float!" | "byte-ptr!" => 8,
            value => {
                let name = value
                    .strip_suffix("! value")
                    .and_then(|n| n.strip_prefix("Az"))
                    .unwrap_or_else(|| panic!("unknown token `{value}`"));
                type_layout(name, ir()).expect("layout").size
            }
        }
    }

    #[test]
    fn a_bool_field_is_one_byte_not_a_32_bit_logic() {
        let fields = alias_fields("FullWindowState");
        assert!(fields.contains(&"window_focused [byte!]"), "{fields:?}");
        let types = &reds()[..reds().find("#import [").unwrap()];
        assert!(
            !types.contains("[logic!]"),
            "a struct field is still logic!"
        );
    }

    #[test]
    fn a_64_bit_integer_field_is_two_32_bit_halves() {
        let s = ir()
            .structs
            .iter()
            .find(|s| s.generic_params.is_empty() && s.fields.iter().any(|f| f.type_name == "u64"))
            .expect("a struct with a u64 field");
        let f = s.fields.iter().find(|f| f.type_name == "u64").unwrap();
        let fields = alias_fields(&s.name);
        assert!(
            fields.contains(&format!("{} [integer!]", f.name).as_str()),
            "{fields:?}"
        );
        assert!(
            fields.contains(&format!("{}_hi [integer!]", f.name).as_str()),
            "{fields:?}"
        );
    }

    #[test]
    fn every_struct_alias_is_exactly_its_c_size() {
        let mut wrong = Vec::new();
        for s in &ir().structs {
            if !super::should_emit_struct(s, &CodegenConfig::c_header()) {
                continue;
            }
            let Some(layout) = type_layout(&s.name, ir()) else {
                continue;
            };
            let size: usize = alias_fields(&s.name).iter().map(|l| token_size(l)).sum();
            if size != layout.size {
                wrong.push(format!("{}: {} bytes, C has {}", s.name, size, layout.size));
            }
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }
}
