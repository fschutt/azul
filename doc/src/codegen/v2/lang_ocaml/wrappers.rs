//! Idiomatic OCaml wrappers: smart-constructed records with
//! `Gc.finalise` finalisers and a nested `Azul` module hierarchy.
//!
//! For every IR struct that has a matching `<TypeName>_delete` C
//! function, we emit:
//!
//! - In the **interface** (`azul.mli`):
//!     - An abstract record type signature, e.g. `type app`.
//!     - A `make_<t>` smart-constructor signature taking the FFI
//!       struct by value and returning the wrapped record.
//!     - A `dispose_<t>` signature for explicit early disposal.
//!     - A `raw_<t>` accessor returning the underlying FFI struct
//!       (for interop with raw `foreign` calls).
//! - In the **implementation** (`azul.ml`):
//!     - The record type itself: `type app = { mutable raw : Az_app
//!       structure; mutable disposed : bool }`.
//!     - The `make_<t>` function which constructs the record and
//!       attaches a `Gc.finalise` finaliser that calls
//!       `az_<type>_delete (Ctypes.addr r.raw)` exactly once and
//!       sets `disposed <- true`.
//!     - `dispose_<t>` — manually invokes the same teardown.
//!
//! In addition, we surface an idiomatic `module Azul` containing one
//! nested module per IR class with `create` / `<method>` /
//! `<static>` / `delete` style entry points. The `Az` prefix is
//! dropped, so users write `Azul.App.create config` instead of
//! `az_app_create config`.

use anyhow::Result;
use std::collections::BTreeSet;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{
    ArgRefKind, CodegenIR, EnumVariantKind, FunctionDef, FunctionKind, StructDef, TypeCategory,
};
use super::functions::ocaml_binding_name;
use super::{
    inner_pointer_form, inner_pointer_form_type, map_type_to_ocaml, map_type_to_ocaml_typ,
    ocaml_ffi_type_name, ocaml_module_name,
    ocaml_wrapper_type_name, sanitize_doc, sanitize_identifier, to_snake_case,
};

// ============================================================================
// Interface (.mli) emission
// ============================================================================

pub fn emit_wrapper_interface(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.line("(* -------------------------------------------------------------------------- *)");
    builder.line("(* Wrapper records (interface).                                                *)");
    builder.line("(* -------------------------------------------------------------------------- *)");
    builder.blank();

    let delete_set = collect_delete_targets(ir);
    for s in &ir.structs {
        if !should_wrap(s, config) {
            continue;
        }
        if !delete_set.contains(s.name.as_str()) {
            continue;
        }
        emit_wrapper_signature(builder, s);
    }
    builder.blank();

    // Polymorphic-variant signatures for tagged unions live in the
    // interface because user code dispatches against them.
    emit_union_variant_interface(builder, ir, config);
    Ok(())
}

pub fn emit_idiomatic_module_interface(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.line("(* -------------------------------------------------------------------------- *)");
    builder.line("(* Idiomatic per-class submodules (interface).                                *)");
    builder.line("(*                                                                            *)");
    builder.line("(* The Dune library name `azul` causes this file to be reachable as the      *)");
    builder.line("(* `Azul` module from the outside; the per-class submodules below appear as  *)");
    builder.line("(* `Azul.App`, `Azul.Window_create_options`, etc.                             *)");
    builder.line("(* -------------------------------------------------------------------------- *)");
    builder.blank();

    let delete_set = collect_delete_targets(ir);
    for s in &ir.structs {
        if !should_wrap(s, config) {
            continue;
        }
        // Even structs that don't get a wrapper record may have
        // static methods worth exposing; emit a minimal module either
        // way when the type has any non-trait functions.
        if !class_has_visible_methods(&s.name, ir) {
            continue;
        }
        emit_module_interface_for_class(builder, s, ir, &delete_set);
    }

    builder.blank();
    Ok(())
}

// ============================================================================
// Implementation (.ml) emission
// ============================================================================

pub fn emit_wrapper_records(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.line("(* -------------------------------------------------------------------------- *)");
    builder.line("(* Wrapper records + Gc.finalise finalisers.                                  *)");
    builder.line("(* -------------------------------------------------------------------------- *)");
    builder.blank();

    let delete_set = collect_delete_targets(ir);
    for s in &ir.structs {
        if !should_wrap(s, config) {
            continue;
        }
        if !delete_set.contains(s.name.as_str()) {
            continue;
        }
        emit_wrapper_record_impl(builder, s);
    }
    Ok(())
}

pub fn emit_idiomatic_module_implementation(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.line("(* -------------------------------------------------------------------------- *)");
    builder.line("(* Idiomatic per-class submodules (implementation).                           *)");
    builder.line("(* -------------------------------------------------------------------------- *)");
    builder.blank();

    // Polymorphic-variant views — must be defined identically to the
    // .mli so the interface matches the implementation. Without these
    // `dune build` fails with
    //   The type az_foo_view is required but not provided.
    emit_union_variant_interface(builder, ir, config);

    let delete_set = collect_delete_targets(ir);
    for s in &ir.structs {
        if !should_wrap(s, config) {
            continue;
        }
        if !class_has_visible_methods(&s.name, ir) {
            continue;
        }
        emit_module_impl_for_class(builder, s, ir, &delete_set);
    }

    builder.blank();
    Ok(())
}

// ============================================================================
// Filters
// ============================================================================

fn should_wrap(s: &StructDef, config: &CodegenConfig) -> bool {
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
            | TypeCategory::DestructorOrClone
            | TypeCategory::GenericTemplate
    )
}

fn collect_delete_targets(ir: &CodegenIR) -> BTreeSet<&str> {
    ir.functions
        .iter()
        .filter(|f| f.kind == FunctionKind::Delete)
        .map(|f| f.class_name.as_str())
        .collect()
}

fn class_has_visible_methods(class_name: &str, ir: &CodegenIR) -> bool {
    ir.functions_for_class(class_name)
        .any(|f| !f.kind.is_trait_function())
}

// ============================================================================
// Wrapper signatures (.mli)
// ============================================================================

fn emit_wrapper_signature(builder: &mut CodeBuilder, s: &StructDef) {
    let wrapper = ocaml_wrapper_type_name(&s.name);
    let ffi = ocaml_ffi_type_name(&s.name);

    if !s.doc.is_empty() {
        for d in &s.doc {
            builder.line(&format!("(* {} *)", sanitize_doc(d)));
        }
    }
    builder.line(&format!("type {}", wrapper));
    builder.line(&format!(
        "val make_{} : {} Ctypes.structure -> {}",
        wrapper, ffi, wrapper
    ));
    builder.line(&format!(
        "val dispose_{} : {} -> unit",
        wrapper, wrapper
    ));
    builder.line(&format!(
        "val raw_{} : {} -> {} Ctypes.structure",
        wrapper, wrapper, ffi
    ));
}

// ============================================================================
// Wrapper records (.ml)
// ============================================================================

fn emit_wrapper_record_impl(builder: &mut CodeBuilder, s: &StructDef) {
    let wrapper = ocaml_wrapper_type_name(&s.name);
    let ffi = ocaml_ffi_type_name(&s.name);
    // The C `_delete` symbol is `Az<TypeName>_delete`; the OCaml-side
    // `foreign` binding is named by `to_snake_case` of that symbol.
    // Delete bindings go through `ocaml_binding_name` (the `ffi_`
    // prefix) so we route through the actual foreign-imported value.
    let delete_binding = ocaml_binding_name(&format!("Az{}_delete", s.name));

    if !s.doc.is_empty() {
        for d in &s.doc {
            builder.line(&format!("(* {} *)", sanitize_doc(d)));
        }
    }
    builder.line(&format!(
        "type {} = {{ mutable raw : {} Ctypes.structure; mutable disposed : bool }}",
        wrapper, ffi
    ));
    builder.line(&format!(
        "let make_{} (raw : {} Ctypes.structure) : {} =",
        wrapper, ffi, wrapper
    ));
    builder.indent();
    builder.line("let r = { raw; disposed = false } in");
    builder.line("Gc.finalise");
    builder.line("  (fun a ->");
    builder.line("     if not a.disposed then begin");
    // _delete usually expects a pointer to the FFI struct.
    builder.line(&format!(
        "       (try {} (Ctypes.addr a.raw) with _ -> ());",
        delete_binding
    ));
    builder.line("       a.disposed <- true");
    builder.line("     end)");
    builder.line("  r;");
    builder.line("r");
    builder.dedent();
    builder.blank();

    builder.line(&format!(
        "let dispose_{} (a : {}) : unit =",
        wrapper, wrapper
    ));
    builder.indent();
    builder.line("if not a.disposed then begin");
    builder.indent();
    builder.line(&format!(
        "(try {} (Ctypes.addr a.raw) with _ -> ());",
        delete_binding
    ));
    builder.line("a.disposed <- true");
    builder.dedent();
    builder.line("end");
    builder.dedent();
    builder.blank();

    builder.line(&format!(
        "let raw_{} (a : {}) : {} Ctypes.structure = a.raw",
        wrapper, wrapper, ffi
    ));
    builder.blank();
}

// ============================================================================
// Idiomatic module surface
// ============================================================================

fn emit_module_interface_for_class(
    builder: &mut CodeBuilder,
    s: &StructDef,
    ir: &CodegenIR,
    delete_set: &BTreeSet<&str>,
) {
    let module_name = ocaml_module_name(&s.name);
    let wrapper = ocaml_wrapper_type_name(&s.name);
    let ffi = ocaml_ffi_type_name(&s.name);
    let has_wrapper = delete_set.contains(s.name.as_str());

    builder.line(&format!("module {} : sig", module_name));
    builder.indent();
    if has_wrapper {
        builder.line(&format!("type t = {}", wrapper));
    } else {
        builder.line(&format!("type t = {} Ctypes.structure", ffi));
    }
    // AzString.to_string is added by emit_module_impl_for_class as a
    // host-string accessor; declare its signature here so the .mli
    // surface matches.
    if s.name == "String" {
        // Use a non-docstring comment to avoid OCaml's "ambiguous
        // documentation comment" warning (escalated to error) — the
        // docstring would attach to either `type t` above or the next
        // val below.
        builder.line("(* Decode the wrapped UTF-8 bytes into an OCaml string. *)");
        builder.line("val to_string : t -> string");
    }
    for func in ir.functions_for_class(&s.name) {
        if func.kind.is_trait_function() {
            continue;
        }
        let sig = build_method_signature(func, ir, has_wrapper, &s.name);
        builder.line(&sig);
    }
    builder.dedent();
    builder.line("end");
    builder.blank();
}

fn emit_module_impl_for_class(
    builder: &mut CodeBuilder,
    s: &StructDef,
    ir: &CodegenIR,
    delete_set: &BTreeSet<&str>,
) {
    let module_name = ocaml_module_name(&s.name);
    let wrapper = ocaml_wrapper_type_name(&s.name);
    let ffi = ocaml_ffi_type_name(&s.name);
    let has_wrapper = delete_set.contains(s.name.as_str());

    builder.line(&format!("module {} = struct", module_name));
    builder.indent();
    if has_wrapper {
        builder.line(&format!("type t = {}", wrapper));
    } else {
        builder.line(&format!("type t = {} Ctypes.structure", ffi));
    }

    // AzString gets a `to_string` helper that decodes the wrapped
    // UTF-8 bytes into an OCaml string. AzString's C-side layout is
    // `{ vec: AzU8Vec }`, AzU8Vec is `{ ptr, len, cap, destructor }`.
    // Field accessors are emitted by lang_ocaml/types.rs:
    // `az_string_field_vec`, `az_u8_vec_field_ptr`, `_field_len`.
    if s.name == "String" {
        // Plain comment, not a docstring — avoids OCaml's ambiguous-
        // documentation warning when both type t and the following let
        // are candidates for attachment.
        builder.line("(* Decode the wrapped UTF-8 bytes into an OCaml string. *)");
        builder.line("let to_string (self : t) : string =");
        builder.indent();
        builder.line("let raw = self.raw in");
        builder.line("let vec = Ctypes.getf raw az_string_field_vec in");
        builder.line("let vec_ptr = Ctypes.getf vec az_u8_vec_field_ptr in");
        builder.line("let vec_len = Unsigned.Size_t.to_int (Ctypes.getf vec az_u8_vec_field_len) in");
        builder.line("if Ctypes.is_null vec_ptr || vec_len = 0 then \"\"");
        builder.line("else Ctypes.string_from_ptr (Ctypes.from_voidp Ctypes.char vec_ptr) ~length:vec_len");
        builder.dedent();
        builder.blank();
    }

    for func in ir.functions_for_class(&s.name) {
        if func.kind.is_trait_function() {
            continue;
        }
        emit_method_impl(builder, func, ir, has_wrapper, &s.name);
    }

    // Phase I.2.8 (OCaml): `equal` + `hash` per module, routed through
    // the codegen-emitted `Az<X>_partialEq` / `Az<X>_hash` exports.
    emit_ocaml_eq_hash_if_supported(builder, s, ir, has_wrapper);

    // Phase I.3.6 (OCaml): `to_string` per module routed through
    // Az<X>_toDbgString.
    emit_ocaml_to_string_if_supported(builder, s, ir, has_wrapper);

    builder.dedent();
    builder.line("end");
    builder.blank();
}

/// Phase I.3.6 (OCaml): emit `to_string` per-module helper routed
/// through `Az<X>_toDbgString`. Decodes the returned AzString via the
/// existing `string_from_ptr` pattern from String.to_string. Skips the
/// String wrapper itself (already has the vec-direct decoder).
fn emit_ocaml_to_string_if_supported(
    builder: &mut CodeBuilder,
    s: &StructDef,
    ir: &CodegenIR,
    has_wrapper: bool,
) {
    if s.name == "String" {
        return;
    }
    let dbg_sym = format!("Az{}_toDbgString", s.name);
    let has_dbg = s.traits.is_debug
        && ir.functions.iter().any(|f| f.c_name == dbg_sym);
    if !has_dbg {
        return;
    }
    // Skip when the user-facing surface already defines `to_string`
    // (e.g. `AzUrl_toString` maps to `Url.to_string : t -> az_string`).
    // We can't override without breaking the .mli signature.
    if ir.functions.iter().any(|f| {
        f.class_name == s.name
            && idiomatic_method_name(&f.method_name) == "to_string"
    }) {
        return;
    }
    let self_t = if has_wrapper { "t.raw" } else { "t" };
    let raw_dbg = ocaml_binding_name(&dbg_sym);
    builder.line(&format!("(* String repr routed through {}. *)", dbg_sym));
    builder.line("let to_string (t : t) : string =");
    builder.indent();
    builder.line(&format!(
        "let __s = {} (Ctypes.addr {}) in",
        raw_dbg, self_t
    ));
    builder.line("let vec = Ctypes.getf __s az_string_field_vec in");
    builder.line("let vec_ptr = Ctypes.getf vec az_u8_vec_field_ptr in");
    builder.line("let vec_len = Unsigned.Size_t.to_int (Ctypes.getf vec az_u8_vec_field_len) in");
    builder.line("if Ctypes.is_null vec_ptr || vec_len = 0 then \"\"");
    builder.line("else Ctypes.string_from_ptr (Ctypes.from_voidp Ctypes.char vec_ptr) ~length:vec_len");
    builder.dedent();
    builder.blank();
}

/// Phase I.2.8 (OCaml): emit `equal` + `hash` module helpers routed
/// through the C-ABI `_partialEq` / `_hash` exports. Pure type-driven.
fn emit_ocaml_eq_hash_if_supported(
    builder: &mut CodeBuilder,
    s: &StructDef,
    ir: &CodegenIR,
    has_wrapper: bool,
) {
    let eq_sym = format!("Az{}_partialEq", s.name);
    let has_eq = s.traits.is_partial_eq
        && ir.functions.iter().any(|f| f.c_name == eq_sym);
    let hash_sym = format!("Az{}_hash", s.name);
    let has_hash = s.traits.is_hash
        && ir.functions.iter().any(|f| f.c_name == hash_sym);

    let self_a = if has_wrapper { "a.raw" } else { "a" };
    let self_b = if has_wrapper { "b.raw" } else { "b" };
    let self_t = if has_wrapper { "t.raw" } else { "t" };
    let raw_eq = ocaml_binding_name(&eq_sym);
    let raw_hash = ocaml_binding_name(&hash_sym);

    if has_eq {
        builder.line(&format!(
            "(* Equality routed through {}. *)",
            eq_sym
        ));
        builder.line("let equal (a : t) (b : t) : bool =");
        builder.indent();
        builder.line(&format!(
            "{} (Ctypes.addr {}) (Ctypes.addr {})",
            raw_eq, self_a, self_b
        ));
        builder.dedent();
        builder.blank();
    }

    if has_hash {
        builder.line(&format!("(* Hash routed through {}. *)", hash_sym));
        builder.line("let hash (t : t) : int =");
        builder.indent();
        builder.line(&format!(
            "Unsigned.UInt64.to_int ({} (Ctypes.addr {}))",
            raw_hash, self_t
        ));
        builder.dedent();
        builder.blank();
    }
}

/// Build the OCaml type signature for a wrapper-side method.
fn build_method_signature(
    func: &FunctionDef,
    ir: &CodegenIR,
    has_wrapper: bool,
    class_name: &str,
) -> String {
    let method_name = idiomatic_method_name(&func.method_name);
    let class_lower = class_name.to_lowercase();
    let is_self_arg = |name: &str| name == "self" || name == class_lower;

    let mut atoms: Vec<String> = Vec::new();
    let takes_self = matches!(
        func.kind,
        FunctionKind::Method | FunctionKind::MethodMut | FunctionKind::DeepCopy
    );
    let _ = has_wrapper; // signature uses `t` regardless of wrapper-ness
    if takes_self {
        atoms.push("t".to_string());
    }
    // Mirror format-call-args: when takes_self, args[0] IS the
    // implicit self; skip it. Without this the val signature would
    // re-declare a `t` parameter for the snake-cased class name arg.
    let iter: Box<dyn Iterator<Item = &super::super::ir::FunctionArg>> = if takes_self
        && !func.args.is_empty()
    {
        Box::new(func.args.iter().skip(1))
    } else {
        Box::new(func.args.iter())
    };
    for a in iter {
        if is_self_arg(&a.name) {
            continue;
        }
        // Auto-string-conversion (type-driven, no method-name allow-
        // list): Owned `String` args surface as plain OCaml `string`
        // at the wrapper signature; impl wraps with `azul_az_string`.
        if a.type_name.trim() == "String"
            && matches!(a.ref_kind, ArgRefKind::Owned)
        {
            atoms.push("string".to_string());
            continue;
        }
        // VAL signature lives in type position — OCaml types apply
        // constructors postfix (`T ptr`, not `ptr T`) and primitive
        // names differ from their Ctypes value-typ counterparts
        // (e.g. `Unsigned.UInt8.t` not `uint8_t`).
        let view = match a.ref_kind {
            ArgRefKind::Owned => map_type_to_ocaml_typ(&a.type_name, ir),
            ArgRefKind::Ref
            | ArgRefKind::RefMut
            | ArgRefKind::Ptr
            | ArgRefKind::PtrMut => inner_pointer_form_type(a.type_name.trim(), ir),
        };
        atoms.push(view);
    }

    let returns_self = func
        .return_type
        .as_deref()
        .map(|r| r.trim() == class_name)
        .unwrap_or(false);
    let return_view = if let Some(r) = &func.return_type {
        let t = r.trim();
        if matches!(t, "" | "void" | "()" | "c_void") {
            "unit".to_string()
        } else if returns_self {
            // Every module exposes a `type t` — use it for return
            // types that match the class. This handles both wrapper
            // forms (`type t = wrapper_record`) and the no-wrapper
            // form (`type t = az_foo Ctypes.structure`). Without the
            // `has_wrapper` branch the mli's val signature would
            // emit the bare FFI name while the impl returns a
            // structure value, raising "values do not match".
            "t".to_string()
        } else {
            map_type_to_ocaml_typ(r, ir)
        }
    } else {
        "unit".to_string()
    };

    if atoms.is_empty() {
        format!("val {} : unit -> {}", method_name, return_view)
    } else {
        format!("val {} : {} -> {}", method_name, atoms.join(" -> "), return_view)
    }
}

fn emit_method_impl(
    builder: &mut CodeBuilder,
    func: &FunctionDef,
    ir: &CodegenIR,
    has_wrapper: bool,
    class_name: &str,
) {
    let method_name = idiomatic_method_name(&func.method_name);
    let class_lower = class_name.to_lowercase();
    let is_self_arg = |name: &str| name == "self" || name == class_lower;

    if !func.doc.is_empty() {
        for d in &func.doc {
            builder.line(&format!("(* {} *)", sanitize_doc(d)));
        }
    }

    let takes_self = matches!(
        func.kind,
        FunctionKind::Method | FunctionKind::MethodMut | FunctionKind::DeepCopy
    );

    // Build the parameter list.
    let mut params: Vec<String> = Vec::new();
    if takes_self {
        // Type-annotate `self` so OCaml resolves the `.raw` field
        // access to THIS wrapper's record rather than the first
        // record-with-`raw`-field it can find globally (all wrappers
        // share the `raw` label).
        params.push(format!("(self : t)"));
    }
    // For instance methods the first IR arg IS the implicit self —
    // skip it regardless of how api.json named it. Same fix the
    // JVM/.NET/Go/Zig/Ruby wrappers landed in earlier phases.
    let mut user_args: Vec<&super::super::ir::FunctionArg> = Vec::new();
    let iter: Box<dyn Iterator<Item = &super::super::ir::FunctionArg>> = if takes_self
        && !func.args.is_empty()
    {
        Box::new(func.args.iter().skip(1))
    } else {
        Box::new(func.args.iter())
    };
    for a in iter {
        if is_self_arg(&a.name) {
            continue;
        }
        user_args.push(a);
    }
    for a in &user_args {
        params.push(sanitize_identifier(&to_snake_case(&a.name)));
    }
    let param_str = if params.is_empty() {
        "()".to_string()
    } else {
        params.join(" ")
    };

    // Build the call expression.
    let raw_binding = ocaml_binding_name(&func.c_name);

    let mut call_args: Vec<String> = Vec::new();
    if takes_self {
        // Detect whether the C function takes self BY VALUE
        // (ref_kind == Owned on args[0]) or BY POINTER. Same pattern
        // the JVM / .NET / Go wrappers use. By-value passes
        // `self.raw` directly; by-pointer uses `Ctypes.addr`.
        let self_by_value = func
            .args
            .first()
            .map(|a| matches!(a.ref_kind, ArgRefKind::Owned))
            .unwrap_or(false);
        let self_expr = if has_wrapper {
            if self_by_value {
                "self.raw".to_string()
            } else {
                "(Ctypes.addr self.raw)".to_string()
            }
        } else if self_by_value {
            "self".to_string()
        } else {
            "(Ctypes.addr self)".to_string()
        };
        call_args.push(self_expr);
    }
    // Auto-string-conversion (mirrors Java/Kotlin/C#/Ruby/Node/Lua):
    // any Owned `String` arg accepts a plain OCaml string; route it
    // through `azul_az_string` (emitted in managed.rs preamble) so the
    // wrapper receives an `az_string Ctypes.structure`. Pure type-
    // driven; no method-name allowlist.
    for a in &user_args {
        let id = sanitize_identifier(&to_snake_case(&a.name));
        if a.type_name.trim() == "String" && matches!(a.ref_kind, ArgRefKind::Owned) {
            call_args.push(format!("(azul_az_string {})", id));
        } else {
            call_args.push(id);
        }
    }
    let call_str = if call_args.is_empty() {
        format!("{} ()", raw_binding)
    } else {
        format!("{} {}", raw_binding, call_args.join(" "))
    };

    let returns_self = func
        .return_type
        .as_deref()
        .map(|r| r.trim() == class_name)
        .unwrap_or(false);

    let body = if returns_self && has_wrapper {
        format!("make_{} ({})", ocaml_wrapper_type_name(class_name), call_str)
    } else {
        call_str
    };

    builder.line(&format!("let {} {} = {}", method_name, param_str, body));
}

// ============================================================================
// Polymorphic-variant signature for tagged unions
// ============================================================================

fn emit_union_variant_interface(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) {
    let mut emitted_header = false;
    for e in &ir.enums {
        if !config.should_include_type(&e.name) {
            continue;
        }
        if !e.is_union {
            continue;
        }
        if !e.generic_params.is_empty() {
            continue;
        }
        if matches!(
            e.category,
            TypeCategory::Recursive
                | TypeCategory::DestructorOrClone
                | TypeCategory::GenericTemplate
        ) {
            continue;
        }
        if !emitted_header {
            builder.line(
                "(* Polymorphic-variant views for tagged-union enums. The actual *)",
            );
            builder.line(
                "(* payload conversion lives in the implementation; here we expose *)",
            );
            builder.line("(* the variant signature for pattern-matching. *)");
            emitted_header = true;
        }
        let view_name = format!("{}_view", ocaml_ffi_type_name(&e.name));
        builder.line(&format!("type {} = ", view_name));
        builder.indent();
        let mut first = true;
        for v in &e.variants {
            let lit = polymorphic_variant_literal(&v.name);
            let line = match &v.kind {
                EnumVariantKind::Unit => format!("`{}", lit),
                EnumVariantKind::Tuple(_) | EnumVariantKind::Struct(_) => {
                    // Payload-bearing variants are surfaced as opaque
                    // ints (offset into the FFI payload). Users go
                    // through the FFI struct directly when they need
                    // typed payload access.
                    format!("`{} of int", lit)
                }
            };
            if first {
                builder.line(&format!("[ {}", line));
                first = false;
            } else {
                builder.line(&format!("| {}", line));
            }
        }
        builder.line("]");
        builder.dedent();
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Pick an idiomatic OCaml method name from the api.json method name.
/// `new` is renamed to `create` (OCaml's `new` is a class-related
/// keyword; even though we don't use OCaml classes, `create` reads
/// better and matches the C# / Ada conventions).
fn idiomatic_method_name(method_name: &str) -> String {
    let snake = to_snake_case(method_name);
    if snake == "new" {
        return "create".to_string();
    }
    sanitize_identifier(&snake)
}

/// Produce a polymorphic-variant tag literal. Backticks come from the
/// caller; this function ensures the tag itself is a valid OCaml
/// identifier (must start uppercase or be an identifier-like token).
fn polymorphic_variant_literal(name: &str) -> String {
    // Polymorphic variants accept any capitalised identifier.
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_uppercase() => name.to_string(),
        Some(c) => {
            let mut out = String::with_capacity(name.len());
            out.extend(c.to_uppercase());
            out.push_str(chars.as_str());
            out
        }
        None => "Empty".to_string(),
    }
}
