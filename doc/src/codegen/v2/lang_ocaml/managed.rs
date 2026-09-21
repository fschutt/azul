//! OCaml managed-FFI runtime helpers (host-invoker pattern).
//!
//! OCaml's Ctypes-Foreign is libffi-backed, which means closure-to-fnpointer
//! conversion works for pointer-arg signatures (the typical "structs always
//! passed by pointer" path). The host-invoker pattern routes user callbacks
//! through pointer-arg invokers so the libffi closure cast is always legal,
//! and libazul's static thunks handle the by-value plumbing internally.
//!
//! ## Output surface (`azul_managed.ml`)
//!
//! Emitted after the `foreign` bindings, the enum modules and the wrapper
//! records (so the helpers below can return records), before the idiomatic
//! per-class modules:
//!
//! 1. **Foreign bindings** for the host-invoker C-ABI exports
//!    (`AzApp_setHostHandleReleaser`, `AzRefAny_newHostHandle`,
//!    `AzRefAny_getHostHandle`, per-kind `AzApp_set<K>Invoker` /
//!    `Az<K>_createFromHostHandle`).
//! 2. **Handle table + releaser** — a `Hashtbl` keyed by `int64` holding either a user callback or
//!    a `RefAny` user value. The releaser closure is pinned in a top-level `let` so it isn't GC'd.
//! 3. **`azul_refany_create` / `azul_refany_get`** and the string helper — defined before the
//!    invokers, which unwrap `RefAny` arguments through them.
//! 4. **Per-kind invoker closures** — one per host-invoker kind. The invoker is the ONLY place that
//!    casts the type-erased handle-table entry (`Obj.magic`); it casts to the exact closure type
//!    [`closure_signature`] describes, which is also the type the public registration helper
//!    (`azul_register_<kind>`) demands, so a mismatch is a compile error in user code rather than a
//!    crash in the invoker.
//! 5. **`azul_<class>_with_layout`** — the IR-derived smart factory for every class matching
//!    [`layout_callback_factory_info`].
//! 6. **`azul_register_<kind>`** — wrap a typed OCaml closure in the matching `Az<Kind>` wrapper.

use std::collections::BTreeSet;

use super::{
    super::{
        generator::CodeBuilder,
        ir::{CallbackTypedefDef, CodegenIR, FunctionArg, TypeCategory},
        managed_host_invoker::{
            has_return, host_invoker_kinds, layout_callback_factory_info, to_snake_case,
            wrapper_name,
        },
        managed_lang_helpers::is_refany_type,
    },
    is_string_type, ocaml_ffi_type_name, ocaml_module_name, ocaml_wrapper_type_name,
    refany_clone_binding, refany_type_name, sanitize_identifier,
};

// ============================================================================
// Naming shared with the idiomatic surface (wrappers.rs)
// ============================================================================

/// The short label of a callback kind: `LayoutCallback` -> `layout`,
/// `ButtonOnClickCallback` -> `button_on_click`, plain `Callback` -> `` (empty).
///
/// One rule, two consumers: the `azul_register_<label>_callback` helper name
/// below and the `?<label>` optional of a smart constructor whose class has a
/// layout-callback factory (`WindowCreateOptions.create ?layout`).
pub fn callback_kind_label(wrapper: &str) -> String {
    let snake = to_snake_case(wrapper);
    match snake.strip_suffix("_callback") {
        Some(prefix) => prefix.to_string(),
        // Every other callback kind is named `<Something>Callback` and keeps
        // `<something>` as its label; the plain one has nothing left after
        // the suffix, so its label is empty and its helper is
        // `azul_register_callback`.
        // allow-api-name: the un-suffixed wrapper has no other marker.
        None if snake == "callback" => String::new(),
        None => snake,
    }
}

/// `azul_register_<label>_callback`; plain `Callback` -> `azul_register_callback`.
pub fn register_fn_name(wrapper: &str) -> String {
    let label = callback_kind_label(wrapper);
    if label.is_empty() {
        "azul_register_callback".to_string()
    } else {
        format!("azul_register_{}_callback", label)
    }
}

/// `azul_<class>_with_layout`: the smart factory of a layout-callback class.
pub fn layout_factory_fn_name(class_name: &str) -> String {
    format!("azul_{}_with_layout", to_snake_case(class_name))
}

// ============================================================================
// Typed closure contract of every host-invoker kind
// ============================================================================

/// How the invoker hands one C-ABI argument (always `const T*` on the wire)
/// to the OCaml closure. Derived from the arg's IR type only.
pub enum InvokerArg {
    /// `RefAny`: passed as a `ref_any` record holding a CLONE of the
    /// engine's argument, so the host function may keep it (store it, hand it
    /// to a widget) past the call. `RefAny.downcast` recovers the OCaml value.
    Model,
    /// A regular struct / tagged union: a typed `Ctypes.ptr` to the C bytes.
    TypedPtr { typ_value: String, ocaml_type: String },
    /// A unit enum: read through the int pointer and decoded to its ADT.
    UnitEnum { module: String, ffi: String },
    /// Anything else (primitives, opaque categories): the raw `unit Ctypes.ptr`.
    Opaque,
}

fn regular(category: TypeCategory) -> bool {
    !matches!(
        category,
        TypeCategory::Recursive
            | TypeCategory::VecRef
            | TypeCategory::DestructorOrClone
            | TypeCategory::GenericTemplate
    )
}

pub fn invoker_arg(arg: &FunctionArg, ir: &CodegenIR) -> InvokerArg {
    let t = arg.type_name.trim();
    if is_refany_type(t, ir) {
        return InvokerArg::Model;
    }
    if let Some(s) = ir.find_struct(t) {
        if regular(s.category) && s.generic_params.is_empty() {
            let ffi = ocaml_ffi_type_name(t);
            return InvokerArg::TypedPtr {
                ocaml_type: format!("{} Ctypes.structure Ctypes.ptr", ffi),
                typ_value: ffi,
            };
        }
    }
    if let Some(e) = ir.find_enum(t) {
        if regular(e.category) && e.generic_params.is_empty() {
            let ffi = ocaml_ffi_type_name(t);
            if e.is_union {
                return InvokerArg::TypedPtr {
                    ocaml_type: format!("{} Ctypes.structure Ctypes.ptr", ffi),
                    typ_value: ffi,
                };
            }
            return InvokerArg::UnitEnum {
                module: ocaml_module_name(t),
                ffi,
            };
        }
    }
    InvokerArg::Opaque
}

/// The OCaml type of one closure parameter.
pub fn invoker_arg_type(a: &InvokerArg) -> String {
    match a {
        InvokerArg::Model => "ref_any".to_string(),
        InvokerArg::TypedPtr { ocaml_type, .. } => ocaml_type.clone(),
        InvokerArg::UnitEnum { module, .. } => format!("{}.t", module),
        InvokerArg::Opaque => "unit Ctypes.ptr".to_string(),
    }
}

/// The expression the invoker passes for its i-th `unit ptr` parameter.
fn invoker_arg_expr(a: &InvokerArg, i: usize, ir: &CodegenIR) -> String {
    match a {
        // The engine owns the `RefAny` it passes in; the closure gets its own
        // reference so the record's finaliser frees a clone, never the
        // engine's value. The clone's `foreign` value comes off the IR.
        InvokerArg::Model => format!(
            "(make_ref_any ({} (Ctypes.from_voidp az_ref_any arg{})))",
            refany_clone_binding(ir).unwrap_or_default(),
            i
        ),
        InvokerArg::TypedPtr { typ_value, .. } => {
            format!("(Ctypes.from_voidp {} arg{})", typ_value, i)
        }
        InvokerArg::UnitEnum { module, ffi } => format!(
            "(match {}.of_int (Ctypes.(!@) (Ctypes.from_voidp {} arg{})) with | Some x -> x | None \
             -> failwith \"invalid {} discriminant\")",
            module, ffi, i, module
        ),
        InvokerArg::Opaque => format!("arg{}", i),
    }
}

/// What the closure returns and how the invoker writes it through the
/// out-pointer. Derived from the typedef's return type only.
pub enum InvokerRet {
    Unit,
    /// A struct returned by value: the closure hands back the raw
    /// `Ctypes.structure` (the idiomatic layer converts records) and the
    /// invoker byte-copies it into the out-pointer; ownership moves to C.
    Struct { ffi: String },
    /// A unit enum: written as the C `int` discriminant.
    UnitEnum { module: String },
    /// Any other scalar: carried as `int`, written as `int32_t`.
    Opaque,
}

pub fn invoker_ret(cb: &CallbackTypedefDef, ir: &CodegenIR) -> InvokerRet {
    if !has_return(cb) {
        return InvokerRet::Unit;
    }
    let t = cb.return_type.as_deref().unwrap_or("").trim();
    if let Some(s) = ir.find_struct(t) {
        if regular(s.category) {
            return InvokerRet::Struct {
                ffi: ocaml_ffi_type_name(t),
            };
        }
    }
    if let Some(e) = ir.find_enum(t) {
        if regular(e.category) {
            if e.is_union {
                return InvokerRet::Struct {
                    ffi: ocaml_ffi_type_name(t),
                };
            }
            return InvokerRet::UnitEnum {
                module: ocaml_module_name(t),
            };
        }
    }
    InvokerRet::Opaque
}

pub fn invoker_ret_type(r: &InvokerRet) -> String {
    match r {
        InvokerRet::Unit => "unit".to_string(),
        InvokerRet::Struct { ffi } => format!("{} Ctypes.structure", ffi),
        InvokerRet::UnitEnum { module } => format!("{}.t", module),
        InvokerRet::Opaque => "int".to_string(),
    }
}

/// The exact OCaml type of a host closure for `cb`, as the invoker calls it
/// and as `azul_register_<kind>` / `azul_<class>_with_layout` demand it:
/// `ref_any -> az_callback_info Ctypes.structure Ctypes.ptr -> Update.t`.
pub fn closure_signature(cb: &CallbackTypedefDef, ir: &CodegenIR) -> String {
    let mut parts: Vec<String> = cb
        .args
        .iter()
        .map(|a| invoker_arg_type(&invoker_arg(a, ir)))
        .collect();
    if parts.is_empty() {
        parts.push("unit".to_string());
    }
    parts.push(invoker_ret_type(&invoker_ret(cb, ir)));
    parts.join(" -> ")
}

/// The OCaml type `azul_register_<kind>` returns: the wrapper record when the
/// kind's wrapper struct owns native memory, else the raw structure.
pub fn wrapper_return_type(wrapper: &str, records: &BTreeSet<&str>) -> String {
    if records.contains(wrapper) {
        ocaml_wrapper_type_name(wrapper)
    } else {
        format!("{} Ctypes.structure", ocaml_ffi_type_name(wrapper))
    }
}

fn wrap_expr(type_name: &str, records: &BTreeSet<&str>, raw_expr: &str) -> String {
    if records.contains(type_name) {
        format!("make_{} ({})", ocaml_wrapper_type_name(type_name), raw_expr)
    } else {
        raw_expr.to_string()
    }
}

// ============================================================================
// Prelude emission
// ============================================================================

/// Emit the managed-FFI prelude. `records` is the set of struct names that
/// have a wrapper record (see `wrappers::record_types`); helpers returning
/// such a struct return its record so the caller's finaliser bookkeeping is
/// uniform.
pub fn emit_managed_prelude(builder: &mut CodeBuilder, ir: &CodegenIR, records: &BTreeSet<&str>) {
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
    builder.line("(* callbacks live in a Hashtbl keyed by int64; the framework's RefAny    *)");
    builder.line("(* destructor clears the entry via AzApp_setHostHandleReleaser.          *)");
    builder.line("(*                                                                       *)");
    builder.line("(* Every funptr is created with ~thread_registration:true so a call from *)");
    builder.line("(* a thread libazul created (RefAny drop on a worker, ThreadCallback)    *)");
    builder.line("(* first registers that thread with the OCaml runtime. The runtime lock  *)");
    builder.line("(* is deliberately NOT taken per call (~runtime_lock stays false): the   *)");
    builder.line("(* main thread enters App.run holding it, so re-acquiring inside a       *)");
    builder.line("(* callback would deadlock. Callbacks are therefore main-thread only.    *)");
    builder.line("(* ─────────────────────────────────────────────────────────────────── *)");
    builder.blank();

    // 1. Foreign bindings for the host-invoker C-ABI exports.
    builder.line("let _az_app_set_host_handle_releaser =");
    builder.indent();
    builder.line("foreign \"AzApp_setHostHandleReleaser\"");
    builder.line(
        "  (Foreign.funptr ~thread_registration:true (uint64_t @-> returning void) @-> returning \
         void)",
    );
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
    builder.line("(* Handle table: maps a uint64 host-handle id to an OCaml value          *)");
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

    // 3. RefAny + string helpers. Emitted BEFORE the invokers, which unwrap
    //    their RefAny arguments through `azul_refany_get`.
    builder.line("(* ───────────────────────────────────────────────────────────────── *)");
    builder.line("(* Public managed-FFI helpers.                                          *)");
    builder.line("(* ───────────────────────────────────────────────────────────────── *)");
    builder.blank();

    // The host-data handle type comes off the IR by category, so this
    // helper keeps naming the right type if api.json renames it.
    let refany = refany_type_name(ir).unwrap_or_default();
    builder.line("(* Wrap an arbitrary OCaml value in an AzRefAny. The value lives in the *)");
    builder.line("(* shared handle table; the destructor clears it on last-clone drop.    *)");
    builder.line(&format!(
        "let azul_refany_create (value : 'a) : {} =",
        wrapper_return_type(refany, records)
    ));
    builder.indent();
    builder.line("let id = _azul_alloc_handle value in");
    builder.line(&wrap_expr(
        refany,
        records,
        "_az_ref_any_new_host_handle (Unsigned.UInt64.of_int64 id)",
    ));
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

    // RefAny.downcast's lookup: only a value RefAny.upcast stored (an
    // exception value `K.Value v` of the key's local exception) is handed out.
    // The table also holds callback closures and, for raw-path users, values
    // of any type; everything that is not an exception-with-argument block
    // reads as "not this type", so a stray RefAny can never be misread.
    builder.line("(* Raised by RefAny.lift when a callback's RefAny holds another type; *)");
    builder.line("(* the invoker logs it (through the callback's info when it can) and *)");
    builder.line("(* leaves the engine's default result - Update.DoNothing, no DOM.    *)");
    builder.line("exception Azul_downcast_failed of string");
    builder.blank();
    builder.line("(* The exception value RefAny.upcast stored under `refany`, if any. *)");
    builder.line("let azul_refany_get_exn (refany : az_ref_any structure ptr) : exn option =");
    builder.indent();
    builder.line("let id = Unsigned.UInt64.to_int64 (_az_ref_any_get_host_handle refany) in");
    builder.line("if Int64.equal id 0L then None");
    builder.line("else");
    builder.line("  match Hashtbl.find_opt _azul_handles id with");
    builder.line("  | Some o when Obj.is_block o && Obj.tag o = 0 && Obj.size o >= 1");
    builder.line("               && Obj.is_block (Obj.field o 0)");
    builder.line("               && Obj.tag (Obj.field o 0) = Obj.object_tag -> Some (Obj.obj o : exn)");
    builder.line("  | _ -> None");
    builder.dedent();
    builder.blank();

    // Generic mark-as-consumed helper for RAW-path users. The generated
    // wrappers never call it: they consume typed records by writing the
    // `disposed` field directly (see wrappers.rs `emit_method_impl`).
    builder.line("(* Mark a wrapper record as consumed (its raw struct moved into a C call). *)");
    builder.line("(* Stops `Gc.finalise` from later calling `<X>_delete` on freed memory.    *)");
    builder.line("(* Only for hand-written raw-path code: every generated method consumes    *)");
    builder.line("(* the records it moves itself. `a` MUST be a `{raw; disposed}` record.    *)");
    builder.line("let azul_consume (a : 'a) : unit =");
    builder.indent();
    builder.line("(* Field 1 of every wrapper record is `mutable disposed : bool`. *)");
    builder.line("Obj.set_field (Obj.repr a) 1 (Obj.repr true)");
    builder.dedent();
    builder.blank();

    // Auto-AzString-conversion helper. Wrapper methods route every Owned
    // `String` arg through this so user code can pass plain OCaml strings.
    builder.line("(* Convert an OCaml string into an AzString Ctypes.structure. Used by *)");
    builder.line("(* every wrapper method whose arg has IR type `String` (Owned).       *)");
    builder.line("let azul_az_string (s : string) : az_string Ctypes.structure =");
    builder.indent();
    builder.line("let len = Stdlib.String.length s in");
    builder.line("let buf = Ctypes.allocate_n Ctypes.char ~count:len in");
    builder.line("Stdlib.String.iteri (fun i c -> Ctypes.(buf +@ i) <-@ c) s;");
    // The IR carries two functions of this exact shape (`(ptr, len) ->
    // String`) - the strict one and the lossy one - so only the name tells
    // them apart, and a host string must never be silently replaced with
    // U+FFFD.
    // allow-api-name: the strict UTF-8 constructor, picked by name.
    let from_utf8 = super::functions::ocaml_binding_name("AzString_fromUtf8");
    builder.line(&format!(
        "{} (Ctypes.to_voidp buf) (Unsigned.Size_t.of_int len)",
        from_utf8
    ));
    builder.dedent();
    builder.blank();

    // 4. Per-kind invoker closures + their setter calls.
    for cb in host_invoker_kinds(ir) {
        emit_per_kind_invoker(builder, cb, ir);
    }

    // 5. Smart <Class>_with_layout factory: built from `_default()` and
    //    stuffs the host-invoker-registered AzLayoutCallback (with ctx
    //    preserved) into the leaf field at info.field_path. The raw
    //    `Az<Class>_create(AzLayoutCallbackType)` C-ABI export discards ctx
    //    because it takes only a fn pointer, so the codegen-emitted
    //    `<Class>.create_raw` cannot carry a host closure.
    //
    //    Build the value directly from `_default()` — that returns a Ctypes
    //    struct backed by OCaml-managed memory containing libazul's
    //    default-initialized bytes (including refcounted heap pointers inside
    //    nested U8Vec / AzString fields). Getting / setting a field gives us a
    //    stable handle into that memory we can navigate to mutate the leaf
    //    callback field in place.
    //
    //    DO NOT use `Ctypes.make` + `<-@ default_struct`: that allocates a
    //    SEPARATE buffer and memcpys the default bytes into it, creating two
    //    aliased copies of the same heap pointers. When libazul later drops one
    //    of the copies, the other becomes invalid (manifested as
    //    `___BUG_IN_CLIENT_OF_LIBMALLOC_POINTER_BEING_FREED_WAS_NOT_ALLOCATED`
    //    inside `<U8Vec as Drop>::drop`).
    for s in &ir.structs {
        let Some(info) = layout_callback_factory_info(s, ir) else {
            continue;
        };
        let Some(cb) = ir
            .callback_typedefs
            .iter()
            .find(|c| wrapper_name(c) == info.callback_wrapper)
        else {
            continue;
        };
        let default_snake = to_snake_case(&info.default_c_name);
        let cb_snake = to_snake_case(&info.callback_wrapper);
        builder.line(&format!(
            "(* Build a {} with a host-invoker-routed {} callback (ctx preserved). *)",
            info.class_name, info.callback_wrapper
        ));
        builder.line(&format!(
            "(* {}.create_raw goes through the Az{}Type raw-fn-pointer path and loses ctx. *)",
            info.class_name, info.callback_wrapper
        ));
        builder.line(&format!(
            "let {} (layout_fn : {})",
            layout_factory_fn_name(&info.class_name),
            closure_signature(cb, ir)
        ));
        builder.indent();
        builder.line(&format!(
            "  : {} =",
            wrapper_return_type(&info.class_name, records)
        ));
        builder.line(&format!("let wco = ffi_{} () in", default_snake));
        builder.line(&format!(
            "let cb = _az_{}_create_from_host_handle",
            cb_snake
        ));
        builder.line("           (Unsigned.UInt64.of_int64 (_azul_alloc_handle layout_fn)) in");
        // Walk the field path: getf each intermediate level, setf the leaf,
        // then setf back up the chain so the byte-copy nested structs
        // propagate. Accessor names are `<ffi typ>_field_<field>` exactly as
        // types.rs emits them.
        let accessor = |struct_name: &str, seg: &str| {
            format!(
                "{}_field_{}",
                ocaml_ffi_type_name(struct_name),
                field_accessor_segment(seg)
            )
        };
        let depth = info.field_path.len();
        let mut parent_var = "wco".to_string();
        let mut parent_struct = info.class_name.clone();
        let mut intermediates: Vec<(String, String, String, String)> = Vec::new();
        for (i, seg) in info
            .field_path
            .iter()
            .enumerate()
            .take(depth.saturating_sub(1))
        {
            let lvl_var = format!("__lvl{}", i);
            builder.line(&format!(
                "let {lvl} = Ctypes.getf {parent} {accessor} in",
                lvl = lvl_var,
                parent = parent_var,
                accessor = accessor(&parent_struct, seg)
            ));
            intermediates.push((
                lvl_var.clone(),
                parent_var.clone(),
                parent_struct.clone(),
                seg.clone(),
            ));
            parent_var = lvl_var;
            parent_struct = info.field_types[i].clone();
        }
        let leaf_field = info
            .field_path
            .last()
            .expect("layout factory has at least one path segment");
        builder.line(&format!(
            "Ctypes.setf {parent} {accessor} cb;",
            parent = parent_var,
            accessor = accessor(&parent_struct, leaf_field)
        ));
        for (lvl_var, parent_var, parent_struct, seg) in intermediates.iter().rev() {
            builder.line(&format!(
                "Ctypes.setf {parent} {accessor} {lvl};",
                parent = parent_var,
                accessor = accessor(parent_struct, seg),
                lvl = lvl_var
            ));
        }
        builder.line(&wrap_expr(&info.class_name, records, "wco"));
        builder.dedent();
        builder.blank();
    }

    // 6. Public per-kind callback registration helpers: a TYPED closure is
    //    stashed in the handle table; the matching libazul
    //    `Az<Kind>_createFromHostHandle(id)` returns the wrapper struct that
    //    the framework's static thunk dispatches through.
    for cb in host_invoker_kinds(ir) {
        let wrapper = wrapper_name(cb);
        let snake = to_snake_case(wrapper);
        builder.line(&format!(
            "(* Wrap a host-side OCaml closure as a `{}` (host-handle routed). *)",
            wrapper
        ));
        builder.line(&format!(
            "let {} (fn_obj : {}) : {} =",
            register_fn_name(wrapper),
            closure_signature(cb, ir),
            wrapper_return_type(wrapper, records)
        ));
        builder.indent();
        builder.line("let id = _azul_alloc_handle fn_obj in");
        builder.line(&wrap_expr(
            wrapper,
            records,
            &format!(
                "_az_{}_create_from_host_handle (Unsigned.UInt64.of_int64 id)",
                snake
            ),
        ));
        builder.dedent();
        builder.blank();
    }
}

fn emit_per_kind_foreigns(builder: &mut CodeBuilder, cb: &CallbackTypedefDef) {
    let wrapper = wrapper_name(cb);
    let snake = to_snake_case(wrapper);
    let invoker_typ = format!("_az_{}_invoker_typ", snake);
    let setter_name = format!("_az_app_set_{}_invoker", snake);
    let create_name = format!("_az_{}_create_from_host_handle", snake);
    let wrapper_ffi = ocaml_ffi_type_name(wrapper);

    // Invoker typ: void (uint64, const A1*, const A2*, ..., R*). All argument
    // pointers are `(ptr void)` on the wire; the invoker casts them to the
    // typed views `invoker_arg` derives.
    let mut typ_parts: Vec<String> = vec!["uint64_t".to_string()];
    for _arg in &cb.args {
        typ_parts.push("(ptr void)".to_string());
    }
    if has_return(cb) {
        typ_parts.push("(ptr void)".to_string());
    }
    builder.line(&format!(
        "let {} = Foreign.funptr ~thread_registration:true ({} @-> returning void)",
        invoker_typ,
        typ_parts.join(" @-> ")
    ));

    builder.line(&format!("let {} =", setter_name));
    builder.indent();
    builder.line(&format!(
        "foreign \"AzApp_set{}Invoker\" ({} @-> returning void)",
        wrapper, invoker_typ
    ));
    builder.dedent();
    builder.blank();

    // Constructor for the wrapper-from-host-handle (returns the wrapper
    // struct by value, hence `az_<wrapper>` not `(ptr az_<wrapper>)`).
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
    let snake = to_snake_case(wrapper);
    let setter_name = format!("_az_app_set_{}_invoker", snake);
    let ret = invoker_ret(cb, ir);
    let args: Vec<InvokerArg> = cb.args.iter().map(|a| invoker_arg(a, ir)).collect();

    builder.line(&format!(
        "(* {} invoker — dispatches to the handle-table entry, typed as *)",
        wrapper
    ));
    builder.line(&format!("(*   {} *)", closure_signature(cb, ir)));
    builder.line(&format!("let _azul_{}_invoker_pin =", snake));
    builder.indent();
    let mut params: Vec<String> = vec!["id".to_string()];
    for i in 0..cb.args.len() {
        params.push(format!("arg{}", i));
    }
    if has_return(cb) {
        params.push("out_ptr".to_string());
    }
    builder.line(&format!("let invoker {} =", params.join(" ")));
    builder.indent();
    // An enum return gets a defined value (discriminant 0) before the user
    // closure runs, so an exception in it can never leave the out-pointer
    // uninitialised.
    match &ret {
        InvokerRet::UnitEnum { .. } | InvokerRet::Opaque => {
            builder.line("let typed_out = Ctypes.from_voidp Ctypes.int32_t out_ptr in");
            builder.line("Ctypes.(typed_out <-@ 0l);");
        }
        InvokerRet::Struct { ffi } => {
            builder.line(&format!(
                "let typed_out = Ctypes.from_voidp {} out_ptr in",
                ffi
            ));
        }
        InvokerRet::Unit => {}
    }
    builder.line("match Hashtbl.find_opt _azul_handles (Unsigned.UInt64.to_int64 id) with");
    builder.line(&format!(
        "| None -> Printf.eprintf \"[azul] {} invoker: unknown host handle %Ld\\n%!\" \
         (Unsigned.UInt64.to_int64 id)",
        wrapper
    ));
    builder.line("| Some fn_obj ->");
    builder.indent();
    builder.line("(try");
    builder.indent();
    let invoke_args: Vec<String> = if args.is_empty() {
        vec!["()".to_string()]
    } else {
        args.iter()
            .enumerate()
            .map(|(i, a)| invoker_arg_expr(a, i, ir))
            .collect()
    };
    let call = format!(
        "(Obj.magic fn_obj : {}) {}",
        closure_signature(cb, ir),
        invoke_args.join(" ")
    );
    match &ret {
        InvokerRet::Unit => builder.line(&call),
        InvokerRet::Struct { .. } => {
            builder.line(&format!("let ret = {} in", call));
            builder.line("Ctypes.(typed_out <-@ ret)");
        }
        InvokerRet::UnitEnum { module } => {
            builder.line(&format!("let ret = {} in", call));
            builder.line(&format!(
                "Ctypes.(typed_out <-@ Int32.of_int ({}.to_int ret))",
                module
            ));
        }
        InvokerRet::Opaque => {
            builder.line(&format!("let ret = {} in", call));
            builder.line("Ctypes.(typed_out <-@ Int32.of_int ret)");
        }
    }
    builder.dedent();
    // A failed RefAny.lift downcast is reported like every other binding
    // reports it: `info.log(Error, "...")` on the first argument whose class
    // has `log(level, message)` (CallbackInfo), else stderr.
    let log_failure = cb
        .args
        .iter()
        .enumerate()
        .find_map(|(i, a)| {
            let t = a.type_name.trim();
            // `log(level, message)` is the only way a callback can report
            // anything to the host app's log, and nothing in the IR marks a
            // method as "this is the logger".
            let f = ir.functions_for_class(t).find(|f| {
                // allow-api-name: the diagnostic sink, found by name + shape.
                f.method_name == "log"
                    && f.args.len() == 3
                    && f.is_receiver_arg(&f.args[0])
                    && is_string_type(ir, &f.args[2].type_name)
            })?;
            let level = f.args[1].type_name.trim();
            ir.find_enum(level)?.variants.iter().find(|v| v.name == "Error")?;
            Some(format!(
                "{} (Ctypes.from_voidp {} arg{}) ({m}.to_int {m}.Error) (azul_az_string msg)",
                super::functions::ocaml_binding_name(&f.c_name),
                ocaml_ffi_type_name(t),
                i,
                m = ocaml_module_name(level),
            ))
        })
        .unwrap_or_else(|| "Printf.eprintf \"[azul][error] %s\\n%!\" msg".to_string());
    builder.line(&format!("with Azul_downcast_failed msg -> {}", log_failure));
    builder.line(&format!(
        "   | e -> Printf.eprintf \"[azul] {} invoker error: %s\\n%!\" (Printexc.to_string e))",
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

/// Mirror of `types.rs::sanitize_field_identifier`: snake-case the IR field
/// name and route through the shared `sanitize_identifier` reserved-word
/// guard. Field accessors are generated as `az_<class_snake>_field_<this>`
/// and must match the bindings emitted in `types.rs`.
fn field_accessor_segment(name: &str) -> String {
    sanitize_identifier(&super::to_snake_case(name))
}
