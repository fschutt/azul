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
        emit_per_kind_invoker(builder, cb, ir);
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

    // Generic mark-as-consumed helper. Used by user code after
    // passing a wrapper by-value into a consuming C function —
    // without this, the wrapper's `Gc.finalise` would later fire
    // `<X>_delete` on memory libazul has already moved/freed,
    // causing a double-free (manifested as a SIGABRT in U8Vec::drop
    // reachable from App.run → MacOSWindow::new_with_options_internal).
    //
    // Every wrapper record we emit has the same shape
    // `{mutable raw; mutable disposed}` — `disposed` is field index 1.
    // We use `Obj.set_field` to flip it without naming the concrete
    // type. Unsafe but uniform; the alternative is one
    // `consume_<type>` per wrapper.
    builder.line("(* Mark a wrapper as consumed (its raw struct moved into a C call). *)");
    builder.line("(* Stops `Gc.finalise` from later calling `<X>_delete` on freed memory. *)");
    builder.line("let azul_consume (a : 'a) : unit =");
    builder.indent();
    builder.line("(* Field 1 of every wrapper record is `mutable disposed : bool`. *)");
    builder.line("Obj.set_field (Obj.repr a) 1 (Obj.repr true)");
    builder.dedent();
    builder.blank();

    // Auto-AzString-conversion helper. Wrapper methods route every
    // Owned `String` arg through this so user code can pass plain
    // OCaml strings directly (`Dom.create_text \"hi\"`). Pure type-
    // driven; the codegen emits the route in `wrappers.rs` based on
    // arg.type_name == \"String\" and ref_kind == Owned.
    builder.line("(* Convert an OCaml string into an AzString Ctypes.structure. *)");
    builder.line("(* Used by every wrapper method whose arg has IR type `String`   *)");
    builder.line("(* and ref_kind Owned. Idempotent on already-AzString values     *)");
    builder.line("(* (those skip this helper at the call site by codegen choice).  *)");
    builder.line("let azul_az_string (s : string) : az_string Ctypes.structure =");
    builder.indent();
    builder.line("let len = Stdlib.String.length s in");
    builder.line("let buf = Ctypes.allocate_n Ctypes.char ~count:len in");
    builder.line("Stdlib.String.iteri (fun i c -> Ctypes.(buf +@ i) <-@ c) s;");
    builder.line("ffi_az_string_from_utf8 (Ctypes.to_voidp buf) (Unsigned.Size_t.of_int len)");
    builder.dedent();
    builder.blank();

    // Smart <Class>_with_layout constructor: built from `_default()`
    // and stuffs the host-invoker-registered AzLayoutCallback (with
    // ctx preserved) into the leaf field at info.field_path. The raw
    // `Az<Class>_create(AzLayoutCallbackType)` C-ABI export discards
    // ctx because it takes only a fn pointer, so we cannot use the
    // codegen-emitted `<Class>.create` for any host-invoker-routed
    // layout.
    //
    // The helper is emitted once per class that
    // [`layout_callback_factory_info`] matches — today that's only
    // `WindowCreateOptions`, but adding another class to api.json
    // with the same shape lights up an extra helper automatically.
    //
    // Build the value directly from `_default()` — that returns a
    // Ctypes struct backed by OCaml-managed memory containing
    // libazul's default-initialized bytes (including refcounted heap
    // pointers inside nested U8Vec / AzString fields). Getting / setting
    // a field gives us a stable handle into that memory we can navigate
    // to mutate the leaf callback field in place.
    //
    // DO NOT use `Ctypes.make` + `<-@ default_struct`: that allocates
    // a SEPARATE buffer and memcpys the default bytes into it,
    // creating two aliased copies of the same heap pointers. When
    // libazul later drops one of the copies, the other becomes
    // invalid. Manifested as
    // `___BUG_IN_CLIENT_OF_LIBMALLOC_POINTER_BEING_FREED_WAS_NOT_ALLOCATED`
    // inside `<U8Vec as Drop>::drop` from
    // MacOSWindow::new_with_options_internal.
    for s in &ir.structs {
        let Some(info) = super::super::managed_host_invoker::layout_callback_factory_info(s, ir)
        else {
            continue;
        };
        let class_snake = to_snake_lower(&info.class_name);
        let default_snake = to_snake_lower(&info.default_c_name);
        let cb_snake = to_snake_lower(&info.callback_wrapper);
        builder.line(&format!(
            "(* Build a {} with a host-invoker-routed *)",
            info.class_name
        ));
        builder.line(&format!(
            "(* {} callback (ctx preserved). Use this instead of *)",
            info.callback_wrapper
        ));
        builder.line(&format!(
            "(* {}.create, which goes through the *)",
            info.class_name
        ));
        builder.line(&format!(
            "(* Az{}Type raw-fn-pointer path and loses ctx. *)",
            info.callback_wrapper
        ));
        builder.line(&format!(
            "let azul_{}_with_layout (layout_fn : 'a)",
            class_snake
        ));
        builder.indent();
        builder.line(&format!(
            "  : az_{} Ctypes.structure =",
            class_snake
        ));
        builder.line(&format!("let wco = ffi_{} () in", default_snake));
        builder.line(&format!(
            "let cb = _az_{}_create_from_host_handle",
            cb_snake
        ));
        builder.line("           (Unsigned.UInt64.of_int64 (_azul_alloc_handle layout_fn)) in");
        // Walk the field path: getf each intermediate level, setf the
        // leaf, then setf back up the chain so the byte-copy nested
        // structs propagate.
        let depth = info.field_path.len();
        let mut parent_var = "wco".to_string();
        let mut parent_struct_snake = class_snake.clone();
        let mut intermediates: Vec<(String, String, String, String)> = Vec::new();
        for (i, seg) in info.field_path.iter().enumerate().take(depth.saturating_sub(1)) {
            let lvl_var = format!("__lvl{}", i);
            let field_accessor =
                format!("az_{}_field_{}", parent_struct_snake, field_accessor_segment(seg));
            builder.line(&format!(
                "let {lvl} = Ctypes.getf {parent} {accessor} in",
                lvl = lvl_var,
                parent = parent_var,
                accessor = field_accessor
            ));
            let next_struct_snake = to_snake_lower(&info.field_types[i]);
            intermediates.push((
                lvl_var.clone(),
                parent_var.clone(),
                parent_struct_snake.clone(),
                seg.clone(),
            ));
            parent_var = lvl_var;
            parent_struct_snake = next_struct_snake;
        }
        let leaf_field = info
            .field_path
            .last()
            .expect("layout factory has at least one path segment");
        let leaf_accessor = format!(
            "az_{}_field_{}",
            parent_struct_snake,
            field_accessor_segment(leaf_field)
        );
        builder.line(&format!(
            "Ctypes.setf {parent} {accessor} cb;",
            parent = parent_var,
            accessor = leaf_accessor
        ));
        // Write each intermediate back into its parent so the
        // byte-copy semantics propagate up to `wco`.
        for (lvl_var, parent_var, parent_struct_snake, seg) in intermediates.iter().rev() {
            let accessor = format!(
                "az_{}_field_{}",
                parent_struct_snake,
                field_accessor_segment(seg)
            );
            builder.line(&format!(
                "Ctypes.setf {parent} {accessor} {lvl};",
                parent = parent_var,
                accessor = accessor,
                lvl = lvl_var
            ));
        }
        builder.line("wco");
        builder.dedent();
        builder.blank();
    }

    // Public per-kind callback registration helpers. Mirror the Lua /
    // Python / Ruby _register_callback dispatch table — a user
    // closure is stashed in the handle table; the matching libazul
    // `Az<Kind>_createFromHostHandle(id)` returns the wrapper struct
    // that the framework's static thunk dispatches through.
    for cb in host_invoker_kinds(ir) {
        let wrapper = wrapper_name(cb);
        let snake = to_snake_lower(wrapper);
        // Strip a trailing `_callback` from the public fn name so e.g.
        // `LayoutCallback` becomes `azul_register_layout` rather than
        // `azul_register_layout_callback_callback`. Plain `Callback`
        // (snake "callback") becomes just `azul_register_callback`.
        let fn_suffix = snake.strip_suffix("_callback").unwrap_or(&snake);
        let fn_name = if fn_suffix.is_empty() {
            "azul_register_callback".to_string()
        } else if fn_suffix == "callback" {
            "azul_register_callback".to_string()
        } else {
            format!("azul_register_{}_callback", fn_suffix)
        };
        builder.line(&format!(
            "(* Wrap a host-side OCaml closure as an `az_{}` struct. *)",
            snake
        ));
        builder.line(&format!(
            "let {} (fn_obj : 'a) : az_{} structure =",
            fn_name, snake
        ));
        builder.indent();
        builder.line("let id = _azul_alloc_handle fn_obj in");
        builder.line(&format!(
            "_az_{}_create_from_host_handle (Unsigned.UInt64.of_int64 id)",
            snake
        ));
        builder.dedent();
        builder.blank();
    }
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

fn emit_per_kind_invoker(builder: &mut CodeBuilder, cb: &CallbackTypedefDef, ir: &CodegenIR) {
    let wrapper = wrapper_name(cb);
    let snake = to_snake_lower(wrapper);
    let setter_name = format!("_az_app_set_{}_invoker", snake);

    // The invoker looks up the user closure from the handle table,
    // calls it, and (if has_return) writes the user's return value
    // through `out_ptr`. The handle table is type-erased via Obj.magic;
    // the user's closure signature must match what we cast it to here.
    //
    // Return handling depends on the callback's return type:
    // - struct returns (e.g. `Dom` from LayoutCallback): user fn returns
    //   the corresponding wrapper record; we extract `.raw` and write
    //   the struct bytes through the typed out-pointer.
    // - enum returns (e.g. `Update` from Callback): user fn returns
    //   an `int`; we write it as `int32_t` through out_ptr.
    // - void returns: ignore.
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
    // Determine the OCaml-level return type the user's closure must
    // produce. Struct returns → user returns the raw `az_<foo>
    // Ctypes.structure` directly (extract `.raw` from the wrapper
    // record). Enum returns → `int`. Void → `unit`. We use raw
    // structs here rather than the wrapper records (`dom`, `update`,
    // etc.) because the records' `type <foo> = { ... }` definitions
    // come *after* this invoker init in the generated file.
    let (ocaml_return, return_kind) = match cb.return_type.as_deref() {
        Some(rt) => {
            let trimmed = rt.trim();
            if ir.find_struct(trimmed).is_some() {
                (
                    format!("az_{} Ctypes.structure", to_snake_lower(trimmed)),
                    "struct",
                )
            } else if ir.find_enum(trimmed).is_some() {
                ("int".to_string(), "enum")
            } else {
                // Primitive returns get carried as int (Update) or skipped.
                ("int".to_string(), "primitive")
            }
        }
        None => ("unit".to_string(), "unit"),
    };

    // Build the closure's full OCaml signature.
    let fn_type = if cb.args.is_empty() {
        if has_return(cb) {
            format!("unit -> {}", ocaml_return)
        } else {
            "unit -> unit".to_string()
        }
    } else {
        let mut parts = Vec::with_capacity(cb.args.len() + 1);
        for _ in &cb.args {
            parts.push("unit Ctypes.ptr".to_string());
        }
        parts.push(if has_return(cb) { ocaml_return.clone() } else { "unit".to_string() });
        parts.join(" -> ")
    };

    if has_return(cb) {
        builder.line(&format!(
            "let ret = (Obj.magic fn_obj : {}) {} in",
            fn_type,
            invoke_args
                .iter()
                .map(|a| a.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        ));
        match return_kind {
            "struct" => {
                let ctype = match cb.return_type.as_deref() {
                    Some(rt) => format!("az_{}", to_snake_lower(rt.trim())),
                    None => "unit".to_string(),
                };
                builder.line(&format!(
                    "let typed_out = Ctypes.from_voidp {} out_ptr in",
                    ctype
                ));
                builder.line("Ctypes.(typed_out <-@ ret)");
            }
            "enum" | "primitive" => {
                // Integer return → write int32_t at out_ptr.
                builder.line("let typed_out = Ctypes.from_voidp Ctypes.int32_t out_ptr in");
                builder.line("Ctypes.(typed_out <-@ Int32.of_int ret)");
            }
            _ => {
                builder.line("let _ = out_ptr in let _ = ret in ()");
            }
        }
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
pub fn emit_managed_interface(builder: &mut CodeBuilder, ir: &CodegenIR) {
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
    builder.line(
        "val azul_consume : 'a -> unit",
    );
    builder.line(
        "val azul_window_create_options_with_layout : 'a -> az_window_create_options Ctypes.structure",
    );
    // Per-kind callback registration helpers (mirror those emitted by
    // emit_managed_module). User passes a host-side closure; we
    // return the Az<Kind> struct (with cb=static-thunk + ctx=host
    // handle) ready to hand to the C ABI.
    for cb in host_invoker_kinds(ir) {
        let wrapper = wrapper_name(cb);
        let snake = to_snake_lower(wrapper);
        let fn_suffix = snake.strip_suffix("_callback").unwrap_or(&snake);
        let fn_name = if fn_suffix.is_empty() || fn_suffix == "callback" {
            "azul_register_callback".to_string()
        } else {
            format!("azul_register_{}_callback", fn_suffix)
        };
        builder.line(&format!(
            "val {} : 'a -> az_{} Ctypes.structure",
            fn_name, snake
        ));
    }
    builder.blank();
}

/// Convert a wrapper name (PascalCase) to snake_case lowercase for use as
/// an OCaml identifier (Foreign / let binding name).
/// Mirror of `types.rs::sanitize_field_identifier`: snake-case the IR
/// field name and route through the shared `sanitize_identifier`
/// reserved-word guard. Used by the WCO smart-factory emit; field
/// accessors are generated as `az_<class_snake>_field_<this>` and must
/// match the bindings emitted in `types.rs`.
fn field_accessor_segment(name: &str) -> String {
    super::sanitize_identifier(&super::to_snake_case(name))
}

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
