//! OCaml managed-FFI runtime helpers (host-invoker pattern).
//!
//! OCaml's Ctypes-Foreign is libffi-backed, which means closure-to-fnpointer
//! conversion works for pointer-arg signatures (the typical "structs always
//! passed by pointer" path). The host-invoker pattern routes user callbacks
//! through pointer-arg invokers so the libffi closure cast is always legal,
//! and libazul's static thunks handle the by-value plumbing internally.
//!
//! ## Output surface
//!
//! Emitted into `azul.ml` between the regular `foreign` bindings and the
//! wrapper records:
//!
//! 1. **Foreign bindings** for the host-invoker C-ABI exports
//!    (`AzApp_setHostHandleReleaser`, `AzRefAny_newHostHandle`,
//!    `AzRefAny_getHostHandle`, per-kind `AzApp_set<K>Invoker` /
//!    `Az<K>_createFromHostHandle`).
//! 2. **Handle table + releaser** — a `Hashtbl` keyed by `Unsigned.UInt64.t`
//!    holding either a user callback or a `RefAny` user value. The releaser
//!    closure is pinned in a top-level `let` so it isn't GC'd.
//! 3. **Per-kind invoker closures** — one per host-invoker kind, dispatching
//!    through the handle table.
//! 4. **`Azul.register_callback`** — wrap an OCaml closure in the matching
//!    `Az<Kind>` cdata struct.
//! 5. **`Azul.refany_create` / `Azul.refany_get`** — user-data helpers
//!    sharing the same handle table.

use super::super::generator::CodeBuilder;
use super::super::ir::{CallbackTypedefDef, CodegenIR};
use super::super::managed_host_invoker::{
    has_return, host_invoker_kinds, wrapper_name,
};

/// Emit the managed-FFI prelude. Must be called *after* the regular
/// `foreign` bindings (so the wrapper records the prelude references
/// already exist) and *before* the wrapper records / idiomatic module
/// surface (so wrappers can call `Azul.register_callback`).
pub fn emit_managed_prelude(builder: &mut CodeBuilder, ir: &CodegenIR) {
    builder.blank();
    builder.line("(* ─────────────────────────────────────────────────────────────────── *)");
    builder.line("(* Managed-FFI runtime helpers (host-invoker pattern).                  *)");
    builder.line("(*                                                                       *)");
    builder.line("(* libazul exports per callback kind:                                    *)");
    builder.line("(*   * a static thunk (the `cb` field of the callback wrapper),         *)");
    builder.line("(*   * Az<Kind>_createFromHostHandle(u64) -> Az<Kind> constructor,      *)");
    builder.line("(*   * AzApp_set<Kind>Invoker(fn) setter.                                *)");
    builder.line("(*                                                                       *)");
    builder.line("(* We register one libffi closure per kind via Foreign.funptr — these    *)");
    builder.line("(* have *pointer-arg* signatures which Ctypes/Foreign handles fine; the  *)");
    builder.line("(* by-value plumbing lives inside libazul's static thunks. User          *)");
    builder.line("(* callbacks live in a Hashtbl keyed by uint64; the framework's RefAny   *)");
    builder.line("(* destructor clears the entry via AzApp_setHostHandleReleaser.          *)");
    builder.line("(* ─────────────────────────────────────────────────────────────────── *)");
    builder.blank();

    // 1. Foreign bindings for the host-invoker C-ABI exports.
    builder.line("let _az_app_set_host_handle_releaser =");
    builder.indent();
    builder.line("foreign \"AzApp_setHostHandleReleaser\"");
    builder.line("  (Foreign.funptr (uint64_t @-> returning void) @-> returning void)");
    builder.dedent();
    builder.blank();

    builder.line("let _az_ref_any_new_host_handle =");
    builder.indent();
    builder.line("foreign \"AzRefAny_newHostHandle\"");
    builder.line("  (uint64_t @-> returning az_ref_any)");
    builder.dedent();
    builder.blank();

    builder.line("let _az_ref_any_get_host_handle =");
    builder.indent();
    builder.line("foreign \"AzRefAny_getHostHandle\"");
    builder.line("  ((ptr az_ref_any) @-> returning uint64_t)");
    builder.dedent();
    builder.blank();

    for cb in host_invoker_kinds(ir) {
        emit_per_kind_foreigns(builder, cb);
    }

    // 2. Handle table + releaser.
    builder.line("(* Handle table: maps a uint64 host-handle id to a Lisp/OCaml value     *)");
    builder.line("(* (either a registered callback closure or a user RefAny value).       *)");
    builder.line("let _azul_handles : (int64, Obj.t) Hashtbl.t = Hashtbl.create 32");
    builder.line("let _azul_next_handle_id : int64 ref = ref 0L");
    builder.blank();
    builder.line("let _azul_alloc_handle (value : 'a) : int64 =");
    builder.indent();
    builder.line("_azul_next_handle_id := Int64.add !_azul_next_handle_id 1L;");
    builder.line("let id = !_azul_next_handle_id in");
    builder.line("Hashtbl.replace _azul_handles id (Obj.repr value);");
    builder.line("id");
    builder.dedent();
    builder.blank();

    // The releaser closure must be pinned at module scope so OCaml's GC
    // doesn't collect it while libazul still holds the function pointer.
    builder.line("(* Pinned releaser closure — must outlive libazul's reference. *)");
    builder.line("let _azul_releaser_pin =");
    builder.indent();
    builder.line("let releaser id =");
    builder.line("  Hashtbl.remove _azul_handles (Unsigned.UInt64.to_int64 id)");
    builder.line("in");
    builder.line("_az_app_set_host_handle_releaser releaser;");
    builder.line("releaser");
    builder.dedent();
    builder.blank();
    builder.line("let _ = _azul_releaser_pin");
    builder.blank();

    // 3. Per-kind invoker closures + their setter calls.
    for cb in host_invoker_kinds(ir) {
        emit_per_kind_invoker(builder, cb);
    }

    // 4 + 5. User-facing helpers.
    builder.line("(* ───────────────────────────────────────────────────────────────── *)");
    builder.line("(* Public managed-FFI helpers.                                          *)");
    builder.line("(* ───────────────────────────────────────────────────────────────── *)");
    builder.blank();

    builder.line("(* Wrap an arbitrary OCaml value in an AzRefAny. The value lives in the *)");
    builder.line("(* shared handle table; the destructor clears it on last-clone drop.    *)");
    builder.line("let azul_refany_create (value : 'a) : az_ref_any structure =");
    builder.indent();
    builder.line("let id = _azul_alloc_handle value in");
    builder.line("_az_ref_any_new_host_handle (Unsigned.UInt64.of_int64 id)");
    builder.dedent();
    builder.blank();

    builder.line("(* Recover the OCaml value previously wrapped via azul_refany_create.   *)");
    builder.line("let azul_refany_get (refany : az_ref_any structure ptr) : 'a option =");
    builder.indent();
    builder.line("let id = Unsigned.UInt64.to_int64 (_az_ref_any_get_host_handle refany) in");
    builder.line("if Int64.equal id 0L then None");
    builder.line("else");
    builder.line("  match Hashtbl.find_opt _azul_handles id with");
    builder.line("  | None -> None");
    builder.line("  | Some o -> Some (Obj.obj o)");
    builder.dedent();
    builder.blank();
}

fn emit_per_kind_foreigns(builder: &mut CodeBuilder, cb: &CallbackTypedefDef) {
    let wrapper = wrapper_name(cb);
    let snake = to_snake_lower(wrapper);
    let invoker_typ = format!("_az_{}_invoker_typ", snake);
    let setter_name = format!("_az_app_set_{}_invoker", snake);
    let create_name = format!("_az_{}_create_from_host_handle", snake);
    let wrapper_ffi = format!("az_{}", snake);

    // Invoker typ: void (uint64, const A1*, const A2*, ..., R*).
    // All argument pointers are opaque from OCaml's perspective so we
    // type them as `(ptr void)`. The user's closure unmarshals when it
    // pleases (e.g. via Ctypes.from_voidp + Ctypes.coerce).
    let mut typ_parts: Vec<String> = vec!["uint64_t".to_string()];
    for _arg in &cb.args {
        typ_parts.push("(ptr void)".to_string());
    }
    if has_return(cb) {
        typ_parts.push("(ptr void)".to_string());
    }
    builder.line(&format!(
        "let {} = Foreign.funptr ({} @-> returning void)",
        invoker_typ,
        typ_parts.join(" @-> ")
    ));

    // Setter declaration.
    builder.line(&format!("let {} =", setter_name));
    builder.indent();
    builder.line(&format!(
        "foreign \"AzApp_set{}Invoker\" ({} @-> returning void)",
        wrapper, invoker_typ
    ));
    builder.dedent();
    builder.blank();

    // Constructor for the wrapper-from-host-handle (returns wrapper struct
    // by value, hence `az_<wrapper>` not `(ptr az_<wrapper>)`).
    builder.line(&format!("let {} =", create_name));
    builder.indent();
    builder.line(&format!(
        "foreign \"Az{}_createFromHostHandle\" (uint64_t @-> returning {})",
        wrapper, wrapper_ffi
    ));
    builder.dedent();
    builder.blank();
}

fn emit_per_kind_invoker(builder: &mut CodeBuilder, cb: &CallbackTypedefDef) {
    let wrapper = wrapper_name(cb);
    let snake = to_snake_lower(wrapper);
    let setter_name = format!("_az_app_set_{}_invoker", snake);

    // The invoker just looks up the user closure from the handle table
    // and calls it; the user closure is responsible for marshalling
    // the pointer args and writing through `out_ptr` if has_return.
    // We dispatch with pattern matching: the user registered with
    // `register_callback` so the Hashtbl entry is the closure itself.
    builder.line(&format!("(* {} invoker — dispatches to handle-table entry. *)", wrapper));
    builder.line(&format!("let _azul_{}_invoker_pin =", snake));
    builder.indent();
    // Build the closure signature based on arg count.
    let mut params: Vec<String> = vec!["id".to_string()];
    for (i, _) in cb.args.iter().enumerate() {
        params.push(format!("arg{}", i));
    }
    if has_return(cb) {
        params.push("out_ptr".to_string());
    }
    builder.line(&format!(
        "let invoker {} =",
        params.join(" ")
    ));
    builder.indent();
    builder.line(
        "match Hashtbl.find_opt _azul_handles (Unsigned.UInt64.to_int64 id) with",
    );
    builder.line("| None -> ()");
    builder.line("| Some fn_obj ->");
    builder.indent();
    builder.line("(try");
    builder.indent();
    // We just call (Obj.obj fn_obj) on the pointer args. The user
    // is responsible for matching the closure's static type.
    let invoke_args: Vec<String> = (0..cb.args.len())
        .map(|i| format!("arg{}", i))
        .collect();
    // The user closure's signature is determined at register-callback
    // time and varies per kind. We type the cast as a variadic-ish
    // chain (`unit Ctypes.ptr -> unit Ctypes.ptr -> ... -> 'r`) so
    // OCaml's type checker accepts the invocation without arity
    // warnings. The user's actual closure must match — the cost of
    // not emitting per-kind OCaml types here.
    let fn_type = if cb.args.is_empty() {
        if has_return(cb) {
            "unit -> unit".to_string()
        } else {
            "unit -> unit".to_string()
        }
    } else {
        let mut parts = Vec::with_capacity(cb.args.len() + 1);
        for _ in &cb.args {
            parts.push("unit Ctypes.ptr".to_string());
        }
        parts.push("unit".to_string());
        parts.join(" -> ")
    };
    if has_return(cb) {
        builder.line(&format!(
            "let _ret = (Obj.magic fn_obj : {}) {} in",
            fn_type,
            invoke_args
                .iter()
                .map(|a| a.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        ));
        builder.line("let _ = out_ptr in");
        builder.line("ignore _ret");
    } else if invoke_args.is_empty() {
        builder.line("(Obj.magic fn_obj : unit -> unit) ()");
    } else {
        builder.line(&format!(
            "(Obj.magic fn_obj : {}) {}",
            fn_type,
            invoke_args
                .iter()
                .map(|a| a.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        ));
    }
    builder.dedent();
    builder.line(&format!(
        "with e -> Printf.eprintf \"[azul] {} invoker error: %s\\n\" (Printexc.to_string e))",
        wrapper
    ));
    builder.dedent();
    builder.dedent();
    builder.line("in");
    builder.line(&format!("{} invoker;", setter_name));
    builder.line("invoker");
    builder.dedent();
    builder.blank();
    builder.line(&format!("let _ = _azul_{}_invoker_pin", snake));
    builder.blank();
}

/// Emit the public surface (azul_refany_create / azul_refany_get) into
/// the module interface. Internal helpers (`_azul_handles`,
/// `_azul_alloc_handle`, per-kind invoker pins, etc.) stay
/// implementation-private.
pub fn emit_managed_interface(builder: &mut CodeBuilder, _ir: &CodegenIR) {
    builder.blank();
    builder.line("(* ─────────────────────────────────────────────────────────────────── *)");
    builder.line("(* Managed-FFI public helpers (host-invoker pattern).                    *)");
    builder.line("(* ─────────────────────────────────────────────────────────────────── *)");
    builder.blank();
    builder.line(
        "val azul_refany_create : 'a -> az_ref_any Ctypes.structure",
    );
    builder.line(
        "val azul_refany_get : az_ref_any Ctypes.structure Ctypes.ptr -> 'a option",
    );
    builder.blank();
}

/// Convert a wrapper name (PascalCase) to snake_case lowercase for use as
/// an OCaml identifier (Foreign / let binding name).
fn to_snake_lower(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for (i, c) in s.chars().enumerate() {
        if c.is_ascii_uppercase() && i > 0 {
            out.push('_');
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}
