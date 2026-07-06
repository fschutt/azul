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
//! See `scripts/RED_FFI_FINDINGS.md` for the full, cited FFI capability audit
//! (verdict: FEASIBLE via Red/System; ALPHA / unverified because no Red
//! toolchain is installed to compile-check the output).
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
//! See `scripts/WIRING_red.md` for the exact edits.

use anyhow::Result;

use super::config::CodegenConfig;
use super::generator::CodeBuilder;
use super::ir::{ArgRefKind, CodegenIR, EnumDef, FunctionDef, StructDef, TypeCategory};
use super::managed_host_invoker::{
    has_return, host_invoker_kinds, managed_c_symbol, wrapper_name,
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
/// opaque blobs (exact sizing is a follow-up — see `RED_FFI_FINDINGS.md`).
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

    // Structs in dependency order.
    let mut structs: Vec<&StructDef> = ir
        .structs
        .iter()
        .filter(|s| should_emit_struct(s, config))
        .collect();
    structs.sort_by_key(|s| s.sort_order);
    for s in structs {
        emit_struct_alias(b, s, ir);
    }

    // Tagged-union enums as opaque blobs (flagged).
    for e in &ir.enums {
        if !config.should_include_type(&e.name) {
            continue;
        }
        if !e.is_union || !e.generic_params.is_empty() {
            continue;
        }
        emit_union_opaque(b, e);
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
        TypeCategory::Recursive
            | TypeCategory::VecRef
            | TypeCategory::GenericTemplate
    )
}

fn emit_unit_enum(b: &mut CodeBuilder, e: &EnumDef) {
    // Unit enums are C `enum`s (int-sized). Emit `#define AzFoo_Bar N`.
    let mut idx: i64 = 0;
    for v in &e.variants {
        b.line(&format!("#define Az{}_{} {}", e.name, v.name, idx));
        idx += 1;
    }
}

fn emit_struct_alias(b: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR) {
    b.line(&format!("Az{}!: alias struct! [", s.name));
    b.indent();
    if s.fields.is_empty() {
        // Red/System has no zero-field struct; give it one padding word so
        // the alias is valid. A truly empty C struct is 0 bytes, but the Az
        // API has no zero-sized by-value types in practice.
        b.line("_pad [integer!]        ;; placeholder (no public fields)");
    }
    for f in &s.fields {
        let ty = field_type_token(&f.type_name, f.ref_kind, ir);
        b.line(&format!("{} [{}]", sanitize_ident(&f.name), ty));
    }
    b.dedent();
    b.line("]");
    b.blank();
}

fn emit_union_opaque(b: &mut CodeBuilder, e: &EnumDef) {
    // TODO2: exact byte size of tagged unions (AzOption*/AzResult*/unions)
    // requires the shared `layout` pass the Fortran/Pascal bindings use.
    // Until that is wired (see WIRING_red.md), emit a pointer-width opaque
    // stand-in and flag it. This is correct for pointer-sized unions and
    // UNDER-sized for larger payloads — the counter demo only round-trips
    // AzUpdate (a unit enum) so it is unaffected.
    b.line(&format!(
        "Az{}!: alias struct! [    ;; TODO2: opaque union — needs exact layout size",
        e.name
    ));
    b.line("    opaque [byte-ptr!]");
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
        FieldRefKind::Owned => map_owned_type(type_name, ir, /*as_field=*/ true),
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
        "i16" | "u16" | "i32" | "u32" | "c_int" | "c_uint" | "GLint" | "GLuint"
        | "GLenum" | "GLbitfield" | "GLsizei" => "integer!".to_string(),
        // 64-bit ints: Red/System's integer! is 32-bit and it lacks a
        // portable int64. Represent as pointer-width so the ABI slot size is
        // right on LP64; VALUE access needs an int64 shim (see FINDINGS).
        "i64" | "u64" | "GLint64" | "GLuint64" => "byte-ptr!".to_string(),
        "f32" | "GLfloat" | "GLclampf" => "float32!".to_string(),
        "f64" | "GLdouble" | "GLclampd" => "float!".to_string(),
        "usize" | "size_t" | "uintptr_t" | "isize" | "ssize_t" | "intptr_t"
        | "GLsizeiptr" | "GLintptr" => "byte-ptr!".to_string(),
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
                if ta.monomorphized_def.is_some() {
                    format!("Az{}! value", t)
                } else {
                    map_owned_type(&ta.target, ir, _as_field)
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
    b.line(&format!("    /local fp [azul-{s}-user!] iid [integer!]", s = snake));
    b.line("][");
    b.indent();
    b.line("iid: as-integer id");
    b.line(&format!("fp: as azul-{s}-user! azul-lookup-cb iid", s = snake));
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
