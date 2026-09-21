//! Pascal (FPC) managed-FFI runtime helpers (host-invoker pattern).
//!
//! libazul never sees a Pascal object: every callback registration hands the
//! engine a `u64` host-handle id (`Az<K>_createFromHostHandle`), and every
//! callback fire comes back through a per-kind `cdecl` invoker stub
//! (`AzApp_set<K>Invoker`) that looks the id up in a unit-global handle table
//! and dispatches to a Pascal object. The engine tells us when a handle is
//! dead through the releaser (`AzApp_setHostHandleReleaser`).
//!
//! ## Output surface
//!
//! Emitted into `azul.pas`, all of it derived from `host_invoker_kinds(ir)`:
//!
//! 1. Interface part A ([`emit_managed_interface`]): the invoker/releaser
//!    procedural types, the hand-declared host-invoker externals (they are
//!    exported by the dll but not part of api.json), one abstract
//!    `TAz<K>Invoker` base class per kind, `azul_refany_create` / `azul_refany_get`.
//! 2. Callback surface ([`emit_callback_surface_types`]), emitted INSIDE the
//!    wrapper `type` block so it can name wrapper classes such as `TDom`:
//!    per kind the user-facing callback types
//!      - `TAz<K>Event`          method pointer, raw `TAzRefAny` model
//!      - `TAz<K>Proc`           plain function, raw `TAzRefAny` model
//!      - `TAz<K>Func<T>`        plain function, typed model `T` + the kind's other args
//!      - `TAz<K>ModelFunc<T>`   plain function, typed model only
//!      - `TAz<K>Ref<T>` / `TAz<K>ModelRef<T>`  `reference to function` twins,
//!        only on FPC >= 3.3.1 / Delphi (`{$IF FPC_FULLVERSION >= 30301}`)
//!    Generic METHODS (`On<Event><T>`) need FPC 3.2.0+ (`GENERIC_METHOD_GUARD`);
//!    every `On<Event>` also takes a ready `TAz<K>Invoker`, the FPC 3.0.x path,
//!    and the two dispatch classes `TAz<K>Wrapper` (Event/Proc) and
//!    `TAz<K>TypedWrapper<T>` (Func/ModelFunc/Ref/ModelRef, does the
//!    `RefAny -> T` downcast and reports a mismatch through the kind's
//!    `*_log` capable argument when it has one).
//! 3. App helper ([`emit_app_helper_types`]): `TAz<App><T>` — generic over the
//!    model class — created from `(Model, Layout)`; owns the root window
//!    options and exposes them as a flattened property view (`Window.Title`,
//!    `Window.Width`, ...). Every name comes from `layout_callback_factory_info`
//!    and an IR scan for the struct that is constructed from a `RefAny` and run
//!    with the layout-factory struct.
//! 4. Interface tail ([`emit_managed_interface_tail`]): `azul_register_<k>`,
//!    string helpers, `azul_refany_current`, and the `az<Variant>` enum
//!    aliases.
//! 5. Implementation ([`emit_managed_implementation`]): the handle table
//!    (critical section, per-slot `Owned` flag), the releaser, the invoker
//!    stubs, all method bodies, and the ABI self-check in `initialization`.
//!
//! ## Ownership rules (the whole point of the `Owned` flag)
//!
//! * Objects the UNIT creates (`TAz<K>Wrapper` / `TAz<K>TypedWrapper<T>`
//!   instances made by the smart setters and by the app helper) are OWNED by
//!   the table and freed by the releaser when the engine drops the last
//!   clone of the callback's context.
//! * The MODEL handed to `TAz<App><T>.Create` is owned the same way
//!   (`azul_refany_create_owned`), so `App.Free` releases it: the engine
//!   drops the app's RefAnys, the releaser fires, the object is freed. That
//!   is what every other binding does - their runtime reclaims the object
//!   when the handle drops - and it is why the Pascal guide no longer asks
//!   for `Model.Free`. Calling it anyway frees the object twice; Pascal has
//!   no way to detect that, so it is a documented rule, not a guarded one.
//! * An object the USER registers directly (`azul_refany_create(value)`) stays
//!   BORROWED: the table never frees it, because the caller may well outlive
//!   the RefAny and still be using it.
//! * Registering the same object twice reuses its id and bumps a per-slot
//!   refcount, and `Owned` is sticky (ORed in), so a borrowed registration
//!   cannot disown a model. The releaser fires once per RefAny group, so the
//!   slot goes away exactly at the last release.

use std::collections::BTreeSet;

use super::{
    super::{
        config::CodegenConfig,
        generator::CodeBuilder,
        ir::{ArgRefKind, CallbackTypedefDef, CodegenIR, FunctionKind, StructDef, TypeCategory},
        managed_host_invoker::{
            arg_name_or_default, has_return, host_invoker_kinds, layout_callback_factory_info,
            wrapper_name,
        },
        managed_lang_helpers::is_refany_type,
    },
    ffi_type_name, map_type_to_pascal, pointer_type_name, record_type_name, sanitize_identifier,
    to_pascal_case,
    wrappers::pascal_class_name,
};

/// Preprocessor guard around everything that needs anonymous methods /
/// function references. FPC 3.2.x has neither (`reference to` is a 3.3.1
/// trunk + Delphi feature), so those overloads are compiled out there.
pub const FUNCREF_GUARD: &str = "{$IF FPC_FULLVERSION >= 30301}";
pub const FUNCREF_GUARD_END: &str = "{$ENDIF}";

/// Preprocessor guard around generic METHODS (`function OnClick<T: class>`).
/// They arrived in FPC 3.2.0; everything else in the unit (generic classes,
/// generic procedural types, `TAz<App><T>`) also compiles on FPC 3.0.x, where
/// the typed callbacks go through the `On<Event>(TAz<K>Invoker)` overload
/// instead: `.OnClick(TAz<K>TypedWrapper<TMyModel>.Create(OnIncrease))`.
pub const GENERIC_METHOD_GUARD: &str = "{$IF FPC_FULLVERSION >= 30200}";

// ============================================================================
// Callback signature model (shared by the managed prelude and wrappers.rs)
// ============================================================================

/// One user-visible callback parameter (everything after the leading
/// `RefAny` model argument).
pub(super) struct CallbackParam {
    /// Pascal identifier used in the emitted signatures.
    pub name: String,
    /// Pascal by-value type (`TAzCallbackInfo`, `csize_t`, ...).
    pub pas_type: String,
    /// Typed pointer to `pas_type`, used to dereference the invoker's
    /// pointer args (`PAzCallbackInfo`, `pcsize_t`, ...).
    pub ptr_type: String,
}

/// The callback's return type in its Pascal spellings.
pub(super) struct CallbackReturn {
    /// Raw record / enum type (`TAzUpdate`, `TAzDom`).
    pub raw: String,
    /// Typed pointer to `raw` (`PAzUpdate`, `PAzDom`).
    pub ptr: String,
    /// Wrapper class when the return type owns native memory (`TDom`).
    pub wrapper: Option<String>,
    /// `Az<R>_createDefault` when the IR has a `Default` for the type; used
    /// as the fallback value on a model-type mismatch or a `nil` return.
    pub default_ctor: Option<String>,
}

/// Everything the emitters need to know about one host-invoker kind.
pub(super) struct CallbackSig {
    /// Wrapper kind name (`ButtonOnClickCallback`).
    pub kind: String,
    /// Name of the leading `RefAny` argument in the invoker stub
    /// (`None` when the kind does not start with a `RefAny` — then no
    /// typed forms are emitted).
    pub model_arg: Option<String>,
    /// Names of ALL invoker stub pointer args, in C order (the stub
    /// signature is `id; <args...>; out_ptr`).
    pub stub_args: Vec<String>,
    /// User-visible parameters after the model.
    pub params: Vec<CallbackParam>,
    pub ret: Option<CallbackReturn>,
    /// `(index into params, C log function, level constant)` when one of the
    /// kind's arguments can log (`Az<T>_log(ptr, level, message)`).
    pub log: Option<(usize, String, String)>,
}

impl CallbackSig {
    /// The `TAz<K>ModelFunc<T>` short form exists when the kind has
    /// arguments beyond the model that the user may want to ignore.
    pub fn has_model_only_form(&self) -> bool {
        self.model_arg.is_some() && !self.params.is_empty()
    }
    /// The Pascal return spelling on user-facing callback types
    /// (wrapper class when there is one).
    pub fn user_return(&self) -> Option<String> {
        self.ret
            .as_ref()
            .map(|r| r.wrapper.clone().unwrap_or_else(|| r.raw.clone()))
    }
    fn ret_clause(&self) -> String {
        match self.user_return() {
            Some(r) => format!(": {}", r),
            None => String::new(),
        }
    }
    fn fn_kw(&self) -> &'static str {
        if self.ret.is_some() {
            "function"
        } else {
            "procedure"
        }
    }
    fn params_clause(&self) -> String {
        self.params
            .iter()
            .map(|p| format!("; {}: {}", p.name, p.pas_type))
            .collect::<String>()
    }
    /// The parameter list of the UNTYPED callback types (`<K>Event` /
    /// `<K>Proc`), parentheses included — empty when the kind has no
    /// parameters at all.
    ///
    /// The leading `Model: TAzRefAny` exists only when the kind HAS a model,
    /// because that is exactly when `call_args` passes one. Two api.json
    /// typedefs take no arguments whatsoever (`RegisterComponentLibraryFn`
    /// returns a ComponentLibrary out of nothing), and declaring a Model
    /// parameter the dispatcher never passes made the declared type and the
    /// `FEvent(...)` call disagree on arity — FPC: "Wrong number of
    /// parameters specified for call to <Procedure Variable>". The typed
    /// `<K>Func<T>` forms below need no such care: they are only emitted
    /// when there IS a model.
    fn untyped_params_clause(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.model_arg.is_some() {
            parts.push("Model: TAzRefAny".to_string());
        }
        parts.extend(
            self.params
                .iter()
                .map(|p| format!("{}: {}", p.name, p.pas_type)),
        );
        if parts.is_empty() {
            // FPC rejects an empty `()` in a procedural TYPE declaration.
            String::new()
        } else {
            format!("({})", parts.join("; "))
        }
    }
}

/// Typed pointer spelling for a by-value Pascal type produced by
/// [`map_type_to_pascal`]: IR records/enums get their `P`-alias, ctypes
/// primitives their `p`-prefixed ctypes pointer, RTL types their RTL pointer.
pub(super) fn pascal_pointer_type(pas_type: &str) -> String {
    if let Some(rest) = pas_type.strip_prefix("TAz") {
        return format!("PAz{}", rest);
    }
    match pas_type {
        "ByteBool" => "PByteBool".to_string(),
        "PtrInt" => "PPtrInt".to_string(),
        "PChar" => "PPChar".to_string(),
        "Pointer" => "PPointer".to_string(),
        // ctypes spells every pointer as `p<ctype>` (pcint32, pcsize_t, ...).
        other if other.starts_with('c') => format!("p{}", other),
        other => format!("P{}", other),
    }
}

/// A user-visible parameter name for the i-th callback arg: the IR name when
/// api.json carries one, else the argument's type (`CallbackInfo`), else a
/// positional `Arg<i>`. Names are made unique within the signature.
fn callback_param_name(cb: &CallbackTypedefDef, idx: usize, ir: &CodegenIR, used: &mut BTreeSet<String>) -> String {
    let from_ir = arg_name_or_default(cb, idx);
    let base = if from_ir.starts_with("_arg") {
        let ty = cb.args[idx].type_name.trim();
        if ir.find_struct(ty).is_some() || ir.find_enum(ty).is_some() {
            ty.to_string()
        } else {
            format!("Arg{}", idx)
        }
    } else {
        to_pascal_case(&from_ir)
    };
    let base = sanitize_identifier(&base);
    let mut name = base.clone();
    let mut n = 2;
    while !used.insert(name.to_ascii_lowercase()) {
        name = format!("{}{}", base, n);
        n += 1;
    }
    name
}

/// Find the `log(level, message)` capability among the callback's params:
/// the first param whose type has an instance method named `log` taking an
/// enum and a `String`. Returns `(param index, C symbol, level constant)`.
fn mismatch_logger(params: &[(String, &str)], ir: &CodegenIR) -> Option<(usize, String, String)> {
    for (i, (_, ty)) in params.iter().enumerate() {
        let Some(f) = ir.functions.iter().find(|f| {
            f.class_name == *ty
                // api.json flags no capability as "the diagnostic channel",
                // and the shape alone (enum level + String message) also
                // matches ordinary methods, so this one entry point has to
                // be named. Everything around it is derived from the IR.
                // allow-api-name: the diagnostic channel's api.json name.
                && f.method_name == "log"
                && matches!(f.kind, FunctionKind::Method | FunctionKind::MethodMut)
                && f.args.len() == 3
                && super::is_string_type(&f.args[2].type_name, ir)
        }) else {
            continue;
        };
        let level_ty = f.args[1].type_name.trim();
        let Some(e) = ir.find_enum(level_ty) else { continue };
        if e.is_union || e.variants.is_empty() {
            continue;
        }
        let variant = e
            .variants
            .iter()
            .find(|v| v.name == "Error")
            .unwrap_or(&e.variants[0]);
        let level = format!("{}_{}", record_type_name(level_ty), sanitize_identifier(&variant.name));
        return Some((i, f.c_name.clone(), level));
    }
    None
}

/// Build the signature model for one host-invoker kind. `targets` is the set
/// of IR struct names that have a wrapper class (see wrappers.rs).
pub(super) fn callback_sig(cb: &CallbackTypedefDef, ir: &CodegenIR, targets: &BTreeSet<String>) -> CallbackSig {
    let mut used: BTreeSet<String> = ["id", "out_ptr", "self", "result"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let mut stub_args = Vec::new();
    let mut params = Vec::new();
    let mut param_types: Vec<(String, &str)> = Vec::new();
    let mut model_arg = None;
    for (i, a) in cb.args.iter().enumerate() {
        if i == 0 && is_refany_type(&a.type_name, ir) && matches!(a.ref_kind, ArgRefKind::Owned) {
            used.insert("model".to_string());
            model_arg = Some("model".to_string());
            stub_args.push("model".to_string());
            continue;
        }
        let name = callback_param_name(cb, i, ir, &mut used);
        // The host invoker receives EVERY argument as a pointer to the
        // VALUE (`const Az<T>*`, see managed_host_invoker::invoker_c_arg_list
        // — it maps the bare type name and ignores the ref kind, because the
        // static thunk on the Rust side does the by-value plumbing). So the
        // user-visible parameter is the by-value type whatever api.json's
        // ref kind says, and the stub dereferences it exactly once.
        //
        // Spelling a `&mut T` argument as a pointer instead asked for a
        // `PP<T>` cast that no unit declares (`MarginBoxCallback`'s
        // `&mut RefAny` produced `PPAzRefAny(RefAny)^`), and handed the user
        // a pointer where every sibling argument is a value.
        let pas_type = map_type_to_pascal(&a.type_name, ir);
        let ptr_type = pascal_pointer_type(&pas_type);
        stub_args.push(name.clone());
        param_types.push((name.clone(), a.type_name.trim()));
        params.push(CallbackParam { name, pas_type, ptr_type });
    }
    let ret = if has_return(cb) {
        let rt = cb.return_type.as_deref().unwrap().trim();
        let raw = map_type_to_pascal(rt, ir);
        let wrapper = if targets.contains(rt) { Some(pascal_class_name(rt)) } else { None };
        let default_ctor = ir
            .functions
            .iter()
            .find(|f| f.class_name == rt && f.kind == FunctionKind::Default)
            .map(|f| f.c_name.clone());
        Some(CallbackReturn { ptr: pascal_pointer_type(&raw), raw, wrapper, default_ctor })
    } else {
        None
    };
    let log = if model_arg.is_some() { mismatch_logger(&param_types, ir) } else { None };
    CallbackSig { kind: wrapper_name(cb).to_string(), model_arg, stub_args, params, ret, log }
}

/// The `procedure Invoke(id: cuint64; a: Pointer; ...; out_ptr: Pointer)`
/// parameter list shared by the invoker proc type, the base class and the
/// stubs.
fn stub_param_list(sig: &CallbackSig, has_ret: bool) -> String {
    let mut parts = vec!["id: cuint64".to_string()];
    for a in &sig.stub_args {
        parts.push(format!("{}: Pointer", a));
    }
    if has_ret {
        parts.push("out_ptr: Pointer".to_string());
    }
    parts.join("; ")
}

// ============================================================================
// Names
// ============================================================================

fn kind_low(kind: &str) -> String {
    kind.to_lowercase()
}
pub(super) fn event_type(kind: &str) -> String {
    format!("TAz{}Event", kind)
}
pub(super) fn proc_type(kind: &str) -> String {
    format!("TAz{}Proc", kind)
}
pub(super) fn func_type(kind: &str) -> String {
    format!("TAz{}Func", kind)
}
pub(super) fn model_func_type(kind: &str) -> String {
    format!("TAz{}ModelFunc", kind)
}
pub(super) fn ref_type(kind: &str) -> String {
    format!("TAz{}Ref", kind)
}
pub(super) fn model_ref_type(kind: &str) -> String {
    format!("TAz{}ModelRef", kind)
}
pub(super) fn wrapper_class(kind: &str) -> String {
    format!("TAz{}Wrapper", kind)
}
pub(super) fn typed_wrapper_class(kind: &str) -> String {
    format!("TAz{}TypedWrapper", kind)
}
pub(super) fn register_fn(kind: &str) -> String {
    format!("azul_register_{}", kind_low(kind))
}
pub(super) fn invoker_class(kind: &str) -> String {
    format!("TAz{}Invoker", kind)
}

// ============================================================================
// Interface part A: invoker plumbing + refany surface
// ============================================================================

/// Emit the interface-section plumbing that does not depend on wrapper
/// classes. Call from inside the `interface` block, after the regular
/// external function imports and before the wrapper `type` block.
pub fn emit_managed_interface(builder: &mut CodeBuilder, ir: &CodegenIR, targets: &BTreeSet<String>) {
    builder.blank();
    builder.line("{ -------------------------------------------------------------------- }");
    builder.line("{ Managed-FFI runtime helpers (host-invoker pattern).                 }");
    builder.line("{ -------------------------------------------------------------------- }");
    builder.blank();

    builder.line("type");
    builder.indent();
    builder.line("{ Raised by the unit on misuse (no model bound, ABI self-check failed). }");
    builder.line("EAzulError = class(Exception);");
    builder.line("TAzHostHandleReleaserProc = procedure(id: cuint64); cdecl;");
    for cb in host_invoker_kinds(ir) {
        let sig = callback_sig(cb, ir, targets);
        builder.line(&format!(
            "TAz{}InvokerProc = procedure({}); cdecl;",
            sig.kind,
            stub_param_list(&sig, sig.ret.is_some())
        ));
    }
    builder.dedent();
    builder.blank();

    // Host-invoker externals. These are exported by libazul (core/src/
    // host_invoker.rs) but not part of api.json, hence declared here.
    builder.line("{ Host-invoker externals (exported by libazul, not part of api.json). }");
    builder.line(
        "procedure AzApp_setHostHandleReleaser(releaser: TAzHostHandleReleaserProc); cdecl; \
         external AzulLib;",
    );
    builder.line("function AzRefAny_newHostHandle(id: cuint64): TAzRefAny; cdecl; external AzulLib;");
    builder.line("function AzRefAny_getHostHandle(refany: PAzRefAny): cuint64; cdecl; external AzulLib;");
    for cb in host_invoker_kinds(ir) {
        let w = wrapper_name(cb);
        builder.line(&format!(
            "procedure AzApp_set{w}Invoker(invoker: TAz{w}InvokerProc); cdecl; external AzulLib;",
            w = w
        ));
        builder.line(&format!(
            "function Az{w}_createFromHostHandle(id: cuint64): TAz{w}; cdecl; external AzulLib;",
            w = w
        ));
    }
    builder.blank();

    // Abstract dispatch base class per kind. Invoke mirrors the C ABI: the
    // handle id, one raw Pointer per callback arg, and an out_ptr the
    // implementation fills when the callback returns a value.
    builder.line("{ Per-kind dispatch base classes: the invoker stub looks the handle up }");
    builder.line("{ and calls Invoke. TAz<K>Wrapper / TAz<K>TypedWrapper<T> below are the }");
    builder.line("{ implementations users normally reach through the On<Event> setters.  }");
    builder.line("type");
    builder.indent();
    for cb in host_invoker_kinds(ir) {
        let sig = callback_sig(cb, ir, targets);
        builder.line(&format!("TAz{}Invoker = class(TObject)", sig.kind));
        builder.line(&format!(
            "  procedure Invoke({}); virtual; abstract;",
            stub_param_list(&sig, sig.ret.is_some())
        ));
        builder.line("end;");
    }
    builder.dedent();
    builder.blank();

    builder.line("{ Wrap a Pascal object in a RefAny. The object is BORROWED: the handle  }");
    builder.line("{ table never frees it, the caller keeps ownership and frees it after  }");
    builder.line("{ the last thing holding the RefAny is gone.                           }");
    builder.line("function azul_refany_create(value: TObject): TAzRefAny;");
    builder.line("{ The same, but the RefAny OWNS the object: when libazul drops the last }");
    builder.line("{ clone of it, the releaser frees `value`. This is how the app helper  }");
    builder.line("{ takes its model, so App.Free is all a program needs; do not Free an  }");
    builder.line("{ object you passed here.                                              }");
    builder.line("function azul_refany_create_owned(value: TObject): TAzRefAny;");
    builder.line("{ Recover the Pascal object behind a RefAny made by either of those    }");
    builder.line("{ (nil for foreign RefAnys).                                           }");
    builder.line("function azul_refany_get(refany: PAzRefAny): TObject;");
    builder.blank();
}

// ============================================================================
// Callback surface: user-facing callback types + dispatch classes
// ============================================================================

/// Emit, INSIDE an open `type` block that already forward-declared the
/// wrapper classes, the user-facing callback types and the dispatch classes
/// for every host-invoker kind.
pub fn emit_callback_surface_types(builder: &mut CodeBuilder, ir: &CodegenIR, targets: &BTreeSet<String>) {
    builder.line("{ Callback types. <K>Event = method pointer, <K>Proc = plain function,  }");
    builder.line("{ both with the raw TAzRefAny model; <K>Func<T> / <K>ModelFunc<T> get   }");
    builder.line("{ the model downcast to T (ModelFunc drops the other arguments). The   }");
    builder.line("{ Ref twins (`reference to function`) need FPC 3.3.1+ or Delphi.      }");
    for cb in host_invoker_kinds(ir) {
        let sig = callback_sig(cb, ir, targets);
        let k = &sig.kind;
        let ret = sig.ret_clause();
        let kw = sig.fn_kw();
        let params = sig.params_clause();
        let untyped = sig.untyped_params_clause();
        builder.line(&format!("{} = {}{}{} of object;", event_type(k), kw, untyped, ret));
        builder.line(&format!("{} = {}{}{};", proc_type(k), kw, untyped, ret));
        if sig.model_arg.is_some() {
            builder.line(&format!("{}<T> = {}(Model: T{}){};", func_type(k), kw, params, ret));
            if sig.has_model_only_form() {
                builder.line(&format!("{}<T> = {}(Model: T){};", model_func_type(k), kw, ret));
            }
            builder.line(FUNCREF_GUARD);
            builder.line(&format!("{}<T> = reference to {}(Model: T{}){};", ref_type(k), kw, params, ret));
            if sig.has_model_only_form() {
                builder.line(&format!("{}<T> = reference to {}(Model: T){};", model_ref_type(k), kw, ret));
            }
            builder.line(FUNCREF_GUARD_END);
        }
        let stub = stub_param_list(&sig, sig.ret.is_some());
        // Untyped dispatcher.
        builder.line(&format!("{} = class(TAz{}Invoker)", wrapper_class(k), k));
        builder.line("private");
        builder.line(&format!("  FEvent: {};", event_type(k)));
        builder.line(&format!("  FProc: {};", proc_type(k)));
        builder.line("public");
        builder.line(&format!("  constructor Create(Callback: {}); overload;", event_type(k)));
        builder.line(&format!("  constructor Create(Callback: {}); overload;", proc_type(k)));
        builder.line(&format!("  procedure Invoke({}); override;", stub));
        builder.line("end;");
        // Typed dispatcher.
        if sig.model_arg.is_some() {
            builder.line(&format!("{}<T: class> = class(TAz{}Invoker)", typed_wrapper_class(k), k));
            builder.line("private");
            builder.line(&format!("  FFunc: {}<T>;", func_type(k)));
            if sig.has_model_only_form() {
                builder.line(&format!("  FModelFunc: {}<T>;", model_func_type(k)));
            }
            builder.line(FUNCREF_GUARD);
            builder.line(&format!("  FRef: {}<T>;", ref_type(k)));
            if sig.has_model_only_form() {
                builder.line(&format!("  FModelRef: {}<T>;", model_ref_type(k)));
            }
            builder.line(FUNCREF_GUARD_END);
            builder.line("public");
            builder.line(&format!("  constructor Create(Callback: {}<T>); overload;", func_type(k)));
            if sig.has_model_only_form() {
                builder.line(&format!("  constructor Create(Callback: {}<T>); overload;", model_func_type(k)));
            }
            builder.line(FUNCREF_GUARD);
            builder.line(&format!("  constructor Create(Callback: {}<T>); overload;", ref_type(k)));
            if sig.has_model_only_form() {
                builder.line(&format!("  constructor Create(Callback: {}<T>); overload;", model_ref_type(k)));
            }
            builder.line(FUNCREF_GUARD_END);
            builder.line(&format!("  procedure Invoke({}); override;", stub));
            builder.line("end;");
        }
    }
    builder.blank();
}

// ============================================================================
// App helper: TAz<App><T> + flattened window-options view
// ============================================================================

/// IR-derived description of the generic app helper.
pub(super) struct AppHelperInfo {
    /// The app struct (`App`).
    pub app_class: String,
    /// `AzApp_create`'s arg list in order, each either the RefAny slot or the
    /// C default-constructor to call for it.
    pub ctor_c_name: String,
    pub ctor_args: Vec<AppCtorArg>,
    /// The run method (`AzApp_run`) and whether its receiver is by value.
    pub run_c_name: String,
    pub run_self_by_value: bool,
    /// The layout-factory struct (`WindowCreateOptions`) and its metadata.
    pub options: super::super::managed_host_invoker::LayoutCallbackFactoryInfo,
    /// `AzWindowCreateOptions_delete`.
    pub options_delete: String,
    /// Name of the property exposing the options view (`Window`).
    pub options_prop: String,
    /// The view class name (`TAzWindowCreateOptionsProps`).
    pub view_class: String,
    /// The generic helper class name (`TAzApp`).
    pub helper_class: String,
    /// Flattened leaf properties of the options struct.
    pub leaves: Vec<OptionsLeaf>,
}

pub(super) enum AppCtorArg {
    Model,
    Default(String),
}

/// One property of the flattened options view.
pub(super) struct OptionsLeaf {
    /// Pascal property name (`Title`).
    pub prop: String,
    /// Dotted Pascal field path from the options record (`window_state.title`).
    pub path: String,
    /// `true` for `String` leaves (exposed as Pascal `string`), else the
    /// scalar type is in `pas_type`.
    pub is_string: bool,
    pub pas_type: String,
}

/// A struct the options view may recurse into: plain settings data. Anything
/// that carries a raw pointer (vectors, handles, `ptr/len/cap` triples that
/// are not categorised as `Vec`) is a resource, not an option, and is
/// skipped as a whole.
fn struct_is_plain_data(s: &StructDef, ir: &CodegenIR) -> bool {
    !matches!(
        s.category,
        TypeCategory::Recursive
            | TypeCategory::VecRef
            | TypeCategory::Vec
            | TypeCategory::CallbackDataPair
            | TypeCategory::CallbackTypedef
            | TypeCategory::Boxed
            | TypeCategory::GenericTemplate
    ) && s.generic_params.is_empty()
        && s.callback_wrapper_info.is_none()
        && !s.fields.is_empty()
        && !s.fields.iter().any(|f| {
            !matches!(f.ref_kind, super::super::ir::FieldRefKind::Owned)
                || matches!(map_type_to_pascal(f.type_name.trim(), ir).as_str(), "Pointer" | "PChar")
        })
}

/// Breadth-first flatten of the options struct: every `String`, primitive
/// or unit-enum leaf reachable through by-value struct fields (never through
/// tagged unions, vectors, pointers or callbacks) becomes one property named
/// after the leaf field. The shallowest occurrence of a name wins; deeper
/// duplicates are skipped. `reserved` holds member names the view already
/// defines.
fn flatten_options(root: &StructDef, ir: &CodegenIR, config: &CodegenConfig, reserved: &BTreeSet<String>) -> Vec<OptionsLeaf> {
    let mut out = Vec::new();
    let mut seen: BTreeSet<String> = reserved.clone();
    let mut queue: std::collections::VecDeque<(&StructDef, String)> = std::collections::VecDeque::new();
    queue.push_back((root, String::new()));
    while let Some((s, prefix)) = queue.pop_front() {
        for f in &s.fields {
            if !matches!(f.ref_kind, super::super::ir::FieldRefKind::Owned) {
                continue;
            }
            let ty = f.type_name.trim();
            let field = sanitize_identifier(&f.name);
            let path = if prefix.is_empty() { field.clone() } else { format!("{}.{}", prefix, field) };
            let prop = sanitize_identifier(&to_pascal_case(&f.name));
            if let Some(child) = ir.find_struct(ty) {
                if !config.should_include_type(ty) {
                    continue;
                }
                if matches!(child.category, TypeCategory::String) {
                    if seen.insert(prop.to_ascii_lowercase()) {
                        out.push(OptionsLeaf { prop, path, is_string: true, pas_type: "string".to_string() });
                    }
                    continue;
                }
                if struct_is_plain_data(child, ir) {
                    queue.push_back((child, path));
                }
                continue;
            }
            if let Some(e) = ir.find_enum(ty) {
                if e.is_union || e.variants.is_empty() || !config.should_include_type(ty) {
                    continue;
                }
                if seen.insert(prop.to_ascii_lowercase()) {
                    out.push(OptionsLeaf { prop, path, is_string: false, pas_type: record_type_name(ty) });
                }
                continue;
            }
            if ir.find_type_alias(ty).is_some() || ir.callback_typedefs.iter().any(|c| c.name == ty) {
                continue;
            }
            let pas = map_type_to_pascal(ty, ir);
            if pas == "Pointer" || pas.starts_with("array") || pas.starts_with('P') {
                continue;
            }
            if seen.insert(prop.to_ascii_lowercase()) {
                out.push(OptionsLeaf { prop, path, is_string: false, pas_type: pas });
            }
        }
    }
    out
}

/// Derive the app helper from the IR: the struct that is constructed from a
/// `RefAny` (plus defaultable config args) and has a `run`-shaped method
/// taking the layout-factory struct by value.
pub(super) fn app_helper_info(ir: &CodegenIR, config: &CodegenConfig, targets: &BTreeSet<String>) -> Option<AppHelperInfo> {
    let options_struct = ir
        .structs
        .iter()
        .find(|s| config.should_include_type(&s.name) && targets.contains(&s.name) && layout_callback_factory_info(s, ir).is_some())?;
    let options = layout_callback_factory_info(options_struct, ir)?;
    let options_delete = ir
        .functions
        .iter()
        .find(|f| f.class_name == options_struct.name && f.kind == FunctionKind::Delete)?
        .c_name
        .clone();
    for s in ir.structs.iter().filter(|s| config.should_include_type(&s.name) && targets.contains(&s.name)) {
        let Some(ctor) = ir.functions_for_class(&s.name).find(|f| {
            f.kind == FunctionKind::Constructor
                && f.args.iter().filter(|a| is_refany_type(&a.type_name, ir) && matches!(a.ref_kind, ArgRefKind::Owned)).count() == 1
                && f.return_type.as_deref().map(|r| r.trim() == s.name).unwrap_or(false)
        }) else {
            continue;
        };
        // The run entry point: void, takes the options struct by value.
        // `add_window(&mut self, ...)` has the same shape; the entry point
        // is the one with the NON-mutating receiver (configuration
        // mutates the app, running only borrows it).
        let run_candidates: Vec<&super::super::ir::FunctionDef> = ir
            .functions_for_class(&s.name)
            .filter(|f| {
                matches!(f.kind, FunctionKind::Method | FunctionKind::MethodMut)
                    && f.return_type.is_none()
                    && f.args.len() == 2
                    && f.args[1].type_name.trim() == options_struct.name
                    && matches!(f.args[1].ref_kind, ArgRefKind::Owned)
            })
            .collect();
        let Some(run) = run_candidates
            .iter()
            .find(|f| f.kind == FunctionKind::Method)
            .or_else(|| run_candidates.first())
            .copied()
        else {
            continue;
        };
        let mut ctor_args = Vec::new();
        let mut ok = true;
        for a in &ctor.args {
            if is_refany_type(&a.type_name, ir) {
                ctor_args.push(AppCtorArg::Model);
                continue;
            }
            let ty = a.type_name.trim();
            let default = ir
                .functions
                .iter()
                .find(|f| f.class_name == ty && f.kind == FunctionKind::Default)
                .or_else(|| ir.functions.iter().find(|f| f.class_name == ty && f.kind == FunctionKind::Constructor && f.args.is_empty()));
            match default {
                Some(d) => ctor_args.push(AppCtorArg::Default(d.c_name.clone())),
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if !ok {
            continue;
        }
        let run_arg = run.args[1].name.trim_start_matches("root_").to_string();
        let options_prop = sanitize_identifier(&to_pascal_case(&run_arg));
        let view_class = format!("TAz{}Props", options_struct.name);
        let reserved: BTreeSet<String> = ["raw", "create", "destroy", "free", "classname", "classtype", "fraw"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let leaves = flatten_options(options_struct, ir, config, &reserved);
        return Some(AppHelperInfo {
            app_class: s.name.clone(),
            ctor_c_name: ctor.c_name.clone(),
            ctor_args,
            run_c_name: run.c_name.clone(),
            run_self_by_value: matches!(run.args[0].ref_kind, ArgRefKind::Owned),
            options,
            options_delete,
            options_prop,
            view_class,
            helper_class: format!("TAz{}", s.name),
            leaves,
        });
    }
    None
}

fn layout_sig<'a>(info: &AppHelperInfo, ir: &'a CodegenIR, targets: &BTreeSet<String>) -> Option<CallbackSig> {
    host_invoker_kinds(ir)
        .find(|cb| wrapper_name(cb) == info.options.callback_wrapper)
        .map(|cb| callback_sig(cb, ir, targets))
}

/// Emit (inside the wrapper `type` block, after the wrapper classes) the
/// options view class and the generic app helper.
pub fn emit_app_helper_types(builder: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig, targets: &BTreeSet<String>) {
    let Some(info) = app_helper_info(ir, config, targets) else {
        return;
    };
    let Some(lsig) = layout_sig(&info, ir, targets) else {
        return;
    };
    let k = &lsig.kind;
    let opt_rec = record_type_name(&info.options.class_name);
    let opt_ptr = pointer_type_name(&info.options.class_name);

    builder.line(&format!("{{ Flattened read/write view over TAz{}: every String / scalar /   }}", info.options.class_name));
    builder.line("{ enum leaf reachable through by-value struct fields is a property     }");
    builder.line("{ named after the field (shallowest wins). Raw gives the whole record. }");
    builder.line(&format!("{} = class(TObject)", info.view_class));
    builder.line("private");
    builder.indent();
    builder.line(&format!("FRaw: {};", opt_ptr));
    for leaf in &info.leaves {
        builder.line(&format!("function Get{}: {};", leaf.prop, leaf.pas_type));
        builder.line(&format!("procedure Set{}(const AValue: {});", leaf.prop, leaf.pas_type));
    }
    builder.dedent();
    builder.line("public");
    builder.indent();
    builder.line(&format!("constructor Create(ARaw: {});", opt_ptr));
    builder.line(&format!("property Raw: {} read FRaw;", opt_ptr));
    for leaf in &info.leaves {
        builder.line(&format!(
            "property {p}: {t} read Get{p} write Set{p};",
            p = leaf.prop,
            t = leaf.pas_type
        ));
    }
    builder.dedent();
    builder.line("end;");
    builder.blank();

    let base = pascal_class_name(&info.app_class);
    builder.line(&format!("{{ {}<T>: the app, generic over the model class. Create takes the    }}", info.helper_class));
    builder.line("{ model OVER (Free the app, not the model), wraps the typed layout     }");
    builder.line(&format!("{{ function, owns the root {} options and runs them. T is checked }}", info.options_prop));
    builder.line("{ on every callback fire.                                              }");
    builder.line(&format!("{{ NOTE: in a mode-objfpc program the bare name {} now means this     }}", info.helper_class));
    builder.line(&format!("{{ generic; reach the raw record through {}.Raw or use mode delphi.  }}", base));
    builder.line(&format!("{}<T: class> = class({})", info.helper_class, base));
    builder.line("private");
    builder.indent();
    builder.line(&format!("FOptions: {};", opt_rec));
    builder.line("FOptionsOwned: Boolean;");
    builder.line(&format!("F{}: {};", info.options_prop, info.view_class));
    builder.line("FModel: T;");
    builder.line(&format!("procedure Init(Model: T; Handler: TAz{}Invoker);", k));
    builder.dedent();
    builder.line("public");
    builder.indent();
    builder.line(&format!("constructor Create(Model: T; Layout: {}<T>); overload;", func_type(k)));
    if lsig.has_model_only_form() {
        builder.line(&format!("constructor Create(Model: T; Layout: {}<T>); overload;", model_func_type(k)));
    }
    builder.line(&format!("constructor Create(Model: T; Layout: {}); overload;", event_type(k)));
    builder.line(&format!("constructor Create(Model: T; Layout: {}); overload;", proc_type(k)));
    builder.line(FUNCREF_GUARD);
    builder.line(&format!("constructor Create(Model: T; Layout: {}<T>); overload;", ref_type(k)));
    if lsig.has_model_only_form() {
        builder.line(&format!("constructor Create(Model: T; Layout: {}<T>); overload;", model_ref_type(k)));
    }
    builder.line(FUNCREF_GUARD_END);
    builder.line("destructor Destroy; override;");
    builder.line(&format!("property {p}: {v} read F{p};", p = info.options_prop, v = info.view_class));
    builder.line("property Model: T read FModel;");
    builder.line("{ Run the app with the configured root window (never returns while the }");
    builder.line("{ window is open).                                                     }");
    builder.line("procedure Run; overload;");
    builder.dedent();
    builder.line("end;");
    builder.blank();
}

fn emit_app_helper_impl(builder: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig, targets: &BTreeSet<String>) {
    let Some(info) = app_helper_info(ir, config, targets) else {
        return;
    };
    let Some(lsig) = layout_sig(&info, ir, targets) else {
        return;
    };
    let k = &lsig.kind;
    let opt_ptr = pointer_type_name(&info.options.class_name);
    let view = &info.view_class;
    let helper = &info.helper_class;

    builder.line(&format!("constructor {}.Create(ARaw: {});", view, opt_ptr));
    builder.line("begin");
    builder.line("  inherited Create;");
    builder.line("  FRaw := ARaw;");
    builder.line("end;");
    builder.blank();
    for leaf in &info.leaves {
        if leaf.is_string {
            builder.line(&format!("function {}.Get{}: string;", view, leaf.prop));
            builder.line("begin");
            builder.line(&format!("  Result := azul_string_to(FRaw^.{});", leaf.path));
            builder.line("end;");
            builder.line(&format!("procedure {}.Set{}(const AValue: string);", view, leaf.prop));
            builder.line("begin");
            builder.line(&format!("  AzString_delete(@FRaw^.{});", leaf.path));
            builder.line(&format!("  FRaw^.{} := azul_string_from(AValue);", leaf.path));
            builder.line("end;");
        } else {
            builder.line(&format!("function {}.Get{}: {};", view, leaf.prop, leaf.pas_type));
            builder.line("begin");
            builder.line(&format!("  Result := FRaw^.{};", leaf.path));
            builder.line("end;");
            builder.line(&format!("procedure {}.Set{}(const AValue: {});", view, leaf.prop, leaf.pas_type));
            builder.line("begin");
            builder.line(&format!("  FRaw^.{} := AValue;", leaf.path));
            builder.line("end;");
        }
    }
    builder.blank();

    // Shared constructor tail.
    let field_path = info.options.field_path.iter().map(|s| sanitize_identifier(s)).collect::<Vec<_>>().join(".");
    builder.line(&format!("procedure {}<T>.Init(Model: T; Handler: TAz{}Invoker);", helper, k));
    builder.line("var data: TAzRefAny;");
    builder.line("begin");
    builder.indent();
    builder.line("FModel := Model;");
    builder.line("{ The app OWNS the model: the handle frees it once libazul has dropped");
    builder.line("  the last clone of this RefAny, which is strictly after the event loop");
    builder.line("  and every callback are gone. App.Free is therefore enough - a program");
    builder.line("  that also calls Model.Free frees it twice. }");
    builder.line("data := azul_refany_create_owned(Model);");
    builder.line("{ Smart On<Event> setters called outside a callback bind to this app. }");
    builder.line("azul_refany_set_default(data);");
    builder.line(&format!("FOptions := {}();", info.options.default_c_name));
    builder.line(&format!("FOptions.{} := {}(Handler);", field_path, register_fn(k)));
    builder.line("FOptionsOwned := True;");
    builder.line(&format!("F{} := {}.Create(@FOptions);", info.options_prop, view));
    let ctor_args: Vec<String> = info
        .ctor_args
        .iter()
        .map(|a| match a {
            AppCtorArg::Model => "data".to_string(),
            AppCtorArg::Default(c) => format!("{}()", c),
        })
        .collect();
    builder.line(&format!("FRaw := {}({});", info.ctor_c_name, ctor_args.join(", ")));
    builder.line("FOwned := True;");
    builder.dedent();
    builder.line("end;");
    builder.blank();

    let mut ctor = |ty: String, guard: bool| {
        if guard {
            builder.line(FUNCREF_GUARD);
        }
        builder.line(&format!("constructor {}<T>.Create(Model: T; Layout: {});", helper, ty));
        builder.line("begin");
        builder.line("  inherited Create;");
        builder.line(&format!("  Init(Model, {}<T>.Create(Layout));", typed_wrapper_class(k)));
        builder.line("end;");
        if guard {
            builder.line(FUNCREF_GUARD_END);
        }
    };
    ctor(format!("{}<T>", func_type(k)), false);
    if lsig.has_model_only_form() {
        ctor(format!("{}<T>", model_func_type(k)), false);
    }
    ctor(format!("{}<T>", ref_type(k)), true);
    if lsig.has_model_only_form() {
        ctor(format!("{}<T>", model_ref_type(k)), true);
    }
    for ty in [event_type(k), proc_type(k)] {
        builder.line(&format!("constructor {}<T>.Create(Model: T; Layout: {});", helper, ty));
        builder.line("begin");
        builder.line("  inherited Create;");
        builder.line(&format!("  Init(Model, {}.Create(Layout));", wrapper_class(k)));
        builder.line("end;");
    }
    builder.blank();

    builder.line(&format!("destructor {}<T>.Destroy;", helper));
    builder.line("begin");
    builder.indent();
    builder.line(&format!("if FOptionsOwned then {}(@FOptions);", info.options_delete));
    builder.line(&format!("F{}.Free;", info.options_prop));
    builder.line("azul_refany_clear_default;");
    builder.line("{ inherited Destroy deletes the app, which drops its RefAnys; the last");
    builder.line("  drop runs the releaser, which frees the model. So the model outlives");
    builder.line("  every callback and the caller needs no Model.Free. }");
    builder.line("inherited Destroy;");
    builder.dedent();
    builder.line("end;");
    builder.blank();

    builder.line(&format!("procedure {}<T>.Run;", helper));
    builder.line("begin");
    builder.indent();
    builder.line("if not FOptionsOwned then");
    builder.line(&format!("  raise EAzulError.Create('{}.Run: the root window options were already consumed');", helper));
    builder.line("FOptionsOwned := False;");
    let recv = if info.run_self_by_value { "FRaw" } else { "@FRaw" };
    builder.line(&format!("{}({}, FOptions);", info.run_c_name, recv));
    builder.dedent();
    builder.line("end;");
    builder.blank();
}

// ============================================================================
// Interface tail: register functions, string helpers, enum aliases
// ============================================================================

/// Emit the remaining interface declarations after the wrapper `type` block.
pub fn emit_managed_interface_tail(builder: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    builder.line("{ Register a dispatcher (owned by the handle table from now on) and get }");
    builder.line("{ the callback struct libazul expects.                                 }");
    for cb in host_invoker_kinds(ir) {
        let w = wrapper_name(cb);
        builder.line(&format!("function {}(handler: TAz{w}Invoker): TAz{w};", register_fn(w), w = w));
    }
    builder.blank();
    builder.line("{ Pascal string <-> TAzString (from: the result is owned by the caller). }");
    builder.line("function azul_string_from(const s: string): TAzString;");
    builder.line("function azul_string_to(const s: TAzString): string;");
    builder.line("{ A clone of the RefAny bound to the currently running callback, or the }");
    builder.line("{ default set below (the app helper sets its model). Raises EAzulError }");
    builder.line("{ when neither exists.                                                 }");
    builder.line("function azul_refany_current: TAzRefAny;");
    builder.line("{ Bind (a clone of) `data` as the fallback for the On<Event> setters /   }");
    builder.line("{ drop it again. FPC forbids generic bodies from touching unit-private }");
    builder.line("{ state, hence these are plain procedures.                             }");
    builder.line("procedure azul_refany_set_default(const data: TAzRefAny);");
    builder.line("procedure azul_refany_clear_default;");
    builder.blank();

    emit_enum_aliases(builder, ir, config);
}

/// `az<Variant>` aliases for every unit-enum variant. Rule: the alias binds
/// to the FIRST enum (api.json order) declaring that variant name; later
/// enums with the same variant keep only their qualified spelling and are
/// listed in a comment next to the alias. Aliases that would collide with
/// another unit identifier are skipped.
fn emit_enum_aliases(builder: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    let mut taken: BTreeSet<String> = BTreeSet::new();
    for f in &ir.functions {
        taken.insert(f.c_name.to_ascii_lowercase());
    }
    for s in &ir.structs {
        taken.insert(record_type_name(&s.name).to_ascii_lowercase());
        taken.insert(ffi_type_name(&s.name).to_ascii_lowercase());
    }
    for e in &ir.enums {
        taken.insert(record_type_name(&e.name).to_ascii_lowercase());
        taken.insert(ffi_type_name(&e.name).to_ascii_lowercase());
    }
    taken.insert("azullib".to_string());

    // alias -> (winning enum, [other enums])
    let mut order: Vec<String> = Vec::new();
    let mut table: std::collections::BTreeMap<String, (String, String, Vec<String>)> = Default::default();
    for e in &ir.enums {
        if !super::types::should_include_enum(e, config) || e.is_union || e.variants.is_empty() {
            continue;
        }
        let t = record_type_name(&e.name);
        for v in &e.variants {
            let variant = sanitize_identifier(&v.name);
            let alias = format!("az{}", variant);
            let key = alias.to_ascii_lowercase();
            if taken.contains(&key) {
                continue;
            }
            let qualified = format!("{}_{}", t, variant);
            match table.get_mut(&key) {
                Some(entry) => entry.2.push(qualified),
                None => {
                    order.push(key.clone());
                    table.insert(key, (alias, qualified, Vec::new()));
                }
            }
        }
    }
    if order.is_empty() {
        return;
    }
    builder.line("{ Short enum spellings: azRefreshDom = TAzUpdate_RefreshDom, ... A name   }");
    builder.line("{ shared by several enums binds to the first enum declaring it (the    }");
    builder.line("{ others are listed in the comment); use the qualified name for those. }");
    builder.line("const");
    builder.indent();
    for key in &order {
        let (alias, qualified, others) = &table[key];
        if others.is_empty() {
            builder.line(&format!("{} = {};", alias, qualified));
        } else {
            builder.line(&format!("{} = {}; {{ also: {} }}", alias, qualified, others.join(", ")));
        }
    }
    builder.dedent();
    builder.blank();
}

// ============================================================================
// Implementation
// ============================================================================

/// Emit the implementation-section bodies. Call from inside the
/// `implementation` block.
pub fn emit_managed_implementation(builder: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig, targets: &BTreeSet<String>) {
    builder.blank();
    builder.line("{ -------------------------------------------------------------------- }");
    builder.line("{ Managed-FFI runtime helpers (implementation).                        }");
    builder.line("{ -------------------------------------------------------------------- }");
    builder.blank();

    builder.line("var");
    builder.indent();
    builder.line("AzulHandles: array of record");
    builder.line("  Id: cuint64;");
    builder.line("  Value: TObject;");
    builder.line("  { Live registrations of this exact object (re-registering reuses the");
    builder.line("    id). The releaser fires once per RefAny group; the slot goes away");
    builder.line("    at the last release. }");
    builder.line("  RefCount: SizeInt;");
    builder.line("  { True for dispatchers the unit created (freed by the releaser),");
    builder.line("    False for user objects passed to azul_refany_create (borrowed). }");
    builder.line("  Owned: Boolean;");
    builder.line("end;");
    builder.line("AzulNextHandleId: cuint64 = 0;");
    builder.line("{ ThreadCallback fires on a worker thread: every table access locks. }");
    builder.line("AzulHandlesLock: TRTLCriticalSection;");
    builder.line("{ Model of the last created app helper (see azul_refany_current). }");
    builder.line("AzulDefaultData: TAzRefAny;");
    builder.line("AzulHasDefaultData: Boolean = False;");
    builder.dedent();
    builder.line("threadvar");
    builder.indent();
    builder.line("{ Borrowed copy of the data argument of the callback running on this");
    builder.line("  thread (set by the invoker stubs around the dispatch). }");
    builder.line("AzulCurrentData: TAzRefAny;");
    builder.line("AzulHasCurrentData: Boolean;");
    builder.dedent();
    builder.blank();

    builder.line("function azul_alloc_handle(value: TObject; owned: Boolean): cuint64;");
    builder.line("var i: SizeInt;");
    builder.line("begin");
    builder.indent();
    builder.line("EnterCriticalSection(AzulHandlesLock);");
    builder.line("try");
    builder.line("  for i := 0 to High(AzulHandles) do");
    builder.line("    if AzulHandles[i].Value = value then");
    builder.line("    begin");
    builder.line("      Inc(AzulHandles[i].RefCount);");
    builder.line("      AzulHandles[i].Owned := AzulHandles[i].Owned or owned;");
    builder.line("      exit(AzulHandles[i].Id);");
    builder.line("    end;");
    builder.line("  Inc(AzulNextHandleId);");
    builder.line("  i := Length(AzulHandles);");
    builder.line("  SetLength(AzulHandles, i + 1);");
    builder.line("  AzulHandles[i].Id := AzulNextHandleId;");
    builder.line("  AzulHandles[i].Value := value;");
    builder.line("  AzulHandles[i].RefCount := 1;");
    builder.line("  AzulHandles[i].Owned := owned;");
    builder.line("  Result := AzulNextHandleId;");
    builder.line("finally");
    builder.line("  LeaveCriticalSection(AzulHandlesLock);");
    builder.line("end;");
    builder.dedent();
    builder.line("end;");
    builder.blank();

    builder.line("function azul_lookup_handle(id: cuint64): TObject;");
    builder.line("var i: SizeInt;");
    builder.line("begin");
    builder.indent();
    builder.line("Result := nil;");
    builder.line("EnterCriticalSection(AzulHandlesLock);");
    builder.line("try");
    builder.line("  for i := 0 to High(AzulHandles) do");
    builder.line("    if AzulHandles[i].Id = id then exit(AzulHandles[i].Value);");
    builder.line("finally");
    builder.line("  LeaveCriticalSection(AzulHandlesLock);");
    builder.line("end;");
    builder.dedent();
    builder.line("end;");
    builder.blank();

    builder.line("procedure azul_releaser_impl(id: cuint64); cdecl;");
    builder.line("var i: SizeInt; v: TObject; owned: Boolean;");
    builder.line("begin");
    builder.indent();
    builder.line("v := nil; owned := False;");
    builder.line("EnterCriticalSection(AzulHandlesLock);");
    builder.line("try");
    builder.line("  for i := 0 to High(AzulHandles) do");
    builder.line("    if AzulHandles[i].Id = id then");
    builder.line("    begin");
    builder.line("      Dec(AzulHandles[i].RefCount);");
    builder.line("      if AzulHandles[i].RefCount > 0 then exit;");
    builder.line("      v := AzulHandles[i].Value;");
    builder.line("      owned := AzulHandles[i].Owned;");
    builder.line("      AzulHandles[i] := AzulHandles[High(AzulHandles)];");
    builder.line("      SetLength(AzulHandles, Length(AzulHandles) - 1);");
    builder.line("      break;");
    builder.line("    end;");
    builder.line("finally");
    builder.line("  LeaveCriticalSection(AzulHandlesLock);");
    builder.line("end;");
    builder.line("{ Free outside the lock: a destructor may touch the table. }");
    builder.line("if owned and (v <> nil) then v.Free;");
    builder.dedent();
    builder.line("end;");
    builder.blank();

    // Invoker stubs.
    for cb in host_invoker_kinds(ir) {
        let sig = callback_sig(cb, ir, targets);
        let has_ret = sig.ret.is_some();
        let mut forward = vec!["id".to_string()];
        forward.extend(sig.stub_args.iter().cloned());
        if has_ret {
            forward.push("out_ptr".to_string());
        }
        builder.line(&format!(
            "procedure azul_{}_invoker_stub({}); cdecl;",
            kind_low(&sig.kind),
            stub_param_list(&sig, has_ret)
        ));
        builder.line("var obj: TObject; prev: TAzRefAny; prevHas: Boolean;");
        builder.line("begin");
        builder.indent();
        builder.line("obj := azul_lookup_handle(id);");
        builder.line(&format!("if (obj = nil) or not (obj is TAz{}Invoker) then exit;", sig.kind));
        if let Some(model) = &sig.model_arg {
            builder.line("prev := AzulCurrentData; prevHas := AzulHasCurrentData;");
            builder.line(&format!("AzulCurrentData := PAzRefAny({})^; AzulHasCurrentData := True;", model));
        }
        // An exception must not escape into the engine (it would end the
        // program). out_ptr is pre-filled with the kind's default, so a
        // failed callback leaves it untouched and only logs.
        builder.line("try");
        builder.line(&format!("  TAz{}Invoker(obj).Invoke({});", sig.kind, forward.join(", ")));
        builder.line("except");
        builder.line("  on E: Exception do");
        builder.line(&format!(
            "    {};",
            report_line(&sig, &format!("'azul: {} raised ' + E.ClassName + ': ' + E.Message", sig.kind))
        ));
        builder.line("else");
        builder.line(&format!(
            "  {};",
            report_line(&sig, &format!("'azul: {} raised a non-Exception object'", sig.kind))
        ));
        builder.line("end;");
        if sig.model_arg.is_some() {
            builder.line("AzulCurrentData := prev; AzulHasCurrentData := prevHas;");
        }
        builder.dedent();
        builder.line("end;");
        builder.blank();
    }

    // Dispatcher bodies.
    for cb in host_invoker_kinds(ir) {
        let sig = callback_sig(cb, ir, targets);
        emit_wrapper_impl(builder, &sig);
        if sig.model_arg.is_some() {
            emit_typed_wrapper_impl(builder, &sig);
        }
        builder.line(&format!("function {}(handler: TAz{k}Invoker): TAz{k};", register_fn(&sig.kind), k = sig.kind));
        builder.line("begin");
        builder.line(&format!("  Result := Az{}_createFromHostHandle(azul_alloc_handle(handler, True));", sig.kind));
        builder.line("end;");
        builder.blank();
    }

    builder.line("function azul_refany_create(value: TObject): TAzRefAny;");
    builder.line("begin");
    builder.line("  Result := AzRefAny_newHostHandle(azul_alloc_handle(value, False));");
    builder.line("end;");
    builder.blank();
    builder.line("function azul_refany_create_owned(value: TObject): TAzRefAny;");
    builder.line("begin");
    builder.line("  { Owned: azul_releaser_impl frees it when the engine drops the last");
    builder.line("    clone of this RefAny - i.e. after every callback that could still");
    builder.line("    fire is gone. The flag is sticky (see azul_alloc_handle), so");
    builder.line("    registering the same object again as borrowed cannot clear it. }");
    builder.line("  Result := AzRefAny_newHostHandle(azul_alloc_handle(value, True));");
    builder.line("end;");
    builder.blank();
    builder.line("function azul_refany_get(refany: PAzRefAny): TObject;");
    builder.line("var id: cuint64;");
    builder.line("begin");
    builder.line("  id := AzRefAny_getHostHandle(refany);");
    builder.line("  if id = 0 then exit(nil);");
    builder.line("  Result := azul_lookup_handle(id);");
    builder.line("end;");
    builder.blank();
    builder.line("function azul_string_from(const s: string): TAzString;");
    builder.line("begin");
    builder.line("  Result := AzString_fromUtf8(PChar(s), Length(s));");
    builder.line("end;");
    builder.blank();
    builder.line("function azul_string_to(const s: TAzString): string;");
    builder.line("begin");
    builder.line("  SetString(Result, s.vec.ptr_, s.vec.len_);");
    builder.line("end;");
    builder.blank();
    builder.line("function azul_refany_current: TAzRefAny;");
    builder.line("begin");
    builder.line("  if AzulHasCurrentData then exit(AzRefAny_clone(@AzulCurrentData));");
    builder.line("  if AzulHasDefaultData then exit(AzRefAny_clone(@AzulDefaultData));");
    builder.line("  raise EAzulError.Create('azul: no model bound - call the On<Event> setter inside a callback or create the app first');");
    builder.line("end;");
    builder.blank();
    builder.line("procedure azul_refany_set_default(const data: TAzRefAny);");
    builder.line("begin");
    builder.line("  azul_refany_clear_default;");
    builder.line("  AzulDefaultData := AzRefAny_clone(@data);");
    builder.line("  AzulHasDefaultData := True;");
    builder.line("end;");
    builder.blank();
    builder.line("procedure azul_refany_clear_default;");
    builder.line("begin");
    builder.line("  if not AzulHasDefaultData then exit;");
    builder.line("  AzulHasDefaultData := False;");
    builder.line("  AzRefAny_delete(@AzulDefaultData);");
    builder.line("end;");
    builder.blank();

    emit_app_helper_impl(builder, ir, config, targets);

    // ABI self-check: every fieldless enum that crosses a host-invoker
    // signature must be a C `int` (4 bytes) — i.e. `{$PACKENUM 4}` must be in
    // effect. (Record sizes are not asserted: the shared `return_c_size`
    // table is not authoritative — see the fix report — and PACKRECORDS C is
    // verified against clang by the size probe instead.) Goes through a
    // local so FPC does not constant-fold the comparison into an
    // "unreachable code" warning.
    builder.line("{ ABI self-check: fieldless enums must be C ints (PACKENUM 4). }");
    builder.line("procedure AzulAbiCheck;");
    builder.line("var n: SizeInt;");
    builder.line("begin");
    builder.indent();
    let mut checked: BTreeSet<String> = BTreeSet::new();
    for cb in host_invoker_kinds(ir) {
        let types = cb
            .args
            .iter()
            .map(|a| a.type_name.trim().to_string())
            .chain(cb.return_type.iter().map(|r| r.trim().to_string()));
        for ty in types {
            let Some(e) = ir.find_enum(&ty) else { continue };
            if e.is_union || e.variants.is_empty() || !checked.insert(ty.clone()) {
                continue;
            }
            let pas = record_type_name(&ty);
            builder.line(&format!("n := SizeOf({});", pas));
            builder.line("if n <> 4 then");
            builder.line(&format!(
                "  raise EAzulError.CreateFmt('azul.pas ABI mismatch: SizeOf({}) = %d, C expects 4 (PACKENUM drift)', [n]);",
                pas
            ));
        }
    }
    builder.dedent();
    builder.line("end;");
    builder.blank();
    builder.line("{ Registers the releaser and every per-kind invoker stub once. }");
    builder.line("procedure AzulHostInvokerInit;");
    builder.line("begin");
    builder.indent();
    // The two host-invoker entry points are written into the generated
    // unit verbatim; their `external` declarations a few hundred lines up
    // spell them the same way. There is no IR item to derive them from -
    // they are the managed-FFI protocol itself, not api.json surface.
    // allow-api-name: the host-invoker registration calls, emitted verbatim.
    builder.line("AzApp_setHostHandleReleaser(@azul_releaser_impl);");
    for cb in host_invoker_kinds(ir) {
        let w = wrapper_name(cb);
        // allow-api-name: ditto, one registration call per callback kind.
        builder.line(&format!("AzApp_set{w}Invoker(@azul_{}_invoker_stub);", kind_low(w), w = w));
    }
    builder.dedent();
    builder.line("end;");
    builder.blank();
}

/// `TAz<K>Wrapper` bodies (Event / Proc dispatch, raw model).
fn emit_wrapper_impl(builder: &mut CodeBuilder, sig: &CallbackSig) {
    let k = &sig.kind;
    let cls = wrapper_class(k);
    for ty in [event_type(k), proc_type(k)] {
        builder.line(&format!("constructor {}.Create(Callback: {});", cls, ty));
        builder.line("begin");
        builder.line("  inherited Create;");
        if ty == event_type(k) {
            builder.line("  FEvent := Callback;");
        } else {
            builder.line("  FProc := Callback;");
        }
        builder.line("end;");
    }
    builder.line(&format!("procedure {}.Invoke({});", cls, stub_param_list(sig, sig.ret.is_some())));
    let call_args = call_args(sig, None);
    if let Some(ret) = &sig.ret {
        builder.line(&format!("var r: {};", sig.user_return().unwrap()));
        builder.line("begin");
        builder.indent();
        builder.line("if Assigned(FEvent) then");
        builder.line(&format!("  r := FEvent({})", call_args));
        builder.line("else if Assigned(FProc) then");
        builder.line(&format!("  r := FProc({})", call_args));
        builder.line("else");
        builder.line("begin");
        builder.indent();
        emit_default_out(builder, ret);
        builder.line("exit;");
        builder.dedent();
        builder.line("end;");
        emit_write_out(builder, ret);
        builder.dedent();
    } else {
        builder.line("begin");
        builder.indent();
        builder.line(&format!("if Assigned(FEvent) then FEvent({})", call_args));
        builder.line(&format!("else if Assigned(FProc) then FProc({});", call_args));
        builder.dedent();
    }
    builder.line("end;");
    builder.blank();
}

/// `TAz<K>TypedWrapper<T>` bodies: downcast, dispatch, write back.
fn emit_typed_wrapper_impl(builder: &mut CodeBuilder, sig: &CallbackSig) {
    let k = &sig.kind;
    let cls = typed_wrapper_class(k);
    let model = sig.model_arg.as_deref().unwrap();
    let mut ctor = |ty: String, field: &str, guard: bool| {
        if guard {
            builder.line(FUNCREF_GUARD);
        }
        builder.line(&format!("constructor {}<T>.Create(Callback: {});", cls, ty));
        builder.line("begin");
        builder.line("  inherited Create;");
        builder.line(&format!("  {} := Callback;", field));
        builder.line("end;");
        if guard {
            builder.line(FUNCREF_GUARD_END);
        }
    };
    ctor(format!("{}<T>", func_type(k)), "FFunc", false);
    if sig.has_model_only_form() {
        ctor(format!("{}<T>", model_func_type(k)), "FModelFunc", false);
    }
    ctor(format!("{}<T>", ref_type(k)), "FRef", true);
    if sig.has_model_only_form() {
        ctor(format!("{}<T>", model_ref_type(k)), "FModelRef", true);
    }

    builder.line(&format!("procedure {}<T>.Invoke({});", cls, stub_param_list(sig, sig.ret.is_some())));
    if let Some(ret) = &sig.ret {
        builder.line(&format!("var obj: TObject; cls: string; r: {};", sig.user_return().unwrap()));
    } else {
        builder.line("var obj: TObject; cls: string;");
    }
    builder.line("begin");
    builder.indent();
    builder.line(&format!("obj := azul_refany_get(PAzRefAny({}));", model));
    builder.line("if not (obj is T) then");
    builder.line("begin");
    builder.indent();
    builder.line("if obj = nil then cls := 'nil' else cls := obj.ClassName;");
    builder.line(&format!(
        "cls := 'azul: {} expected a model of class ' + T.ClassName + ', got ' + cls;",
        k
    ));
    builder.line(&format!("{};", report_line(sig, "cls")));
    if let Some(ret) = &sig.ret {
        emit_default_out(builder, ret);
    }
    builder.line("exit;");
    builder.dedent();
    builder.line("end;");
    let full = call_args(sig, Some("T(obj)"));
    let assign = if sig.ret.is_some() { "r := " } else { "" };
    builder.line("if Assigned(FFunc) then");
    builder.line(&format!("  {}FFunc({})", assign, full));
    if sig.has_model_only_form() {
        builder.line("else if Assigned(FModelFunc) then");
        builder.line(&format!("  {}FModelFunc(T(obj))", assign));
    }
    builder.line(FUNCREF_GUARD);
    builder.line("else if Assigned(FRef) then");
    builder.line(&format!("  {}FRef({})", assign, full));
    if sig.has_model_only_form() {
        builder.line("else if Assigned(FModelRef) then");
        builder.line(&format!("  {}FModelRef(T(obj))", assign));
    }
    builder.line(FUNCREF_GUARD_END);
    builder.line("else");
    builder.line("begin");
    builder.indent();
    if let Some(ret) = &sig.ret {
        emit_default_out(builder, ret);
    }
    builder.line("exit;");
    builder.dedent();
    builder.line("end;");
    if let Some(ret) = &sig.ret {
        emit_write_out(builder, ret);
    }
    builder.dedent();
    builder.line("end;");
    builder.blank();
}

/// One statement that reports `msg` (a Pascal string expression) for a
/// callback of this kind: through the kind's `*_log(ptr, Error, message)`
/// capable argument when it has one (`CallbackInfo`), else on stderr.
fn report_line(sig: &CallbackSig, msg: &str) -> String {
    match &sig.log {
        Some((idx, c_fn, level)) => {
            let p = &sig.params[*idx];
            format!("{}({}({}), {}, azul_string_from({}))", c_fn, p.ptr_type, p.name, level, msg)
        }
        None => format!("WriteLn(StdErr, {})", msg),
    }
}

/// The argument list handed to a user callback: the model expression (raw
/// RefAny copy by default), then every other invoker pointer arg
/// dereferenced through its typed pointer.
fn call_args(sig: &CallbackSig, model_expr: Option<&str>) -> String {
    let mut parts = Vec::new();
    if let Some(model) = &sig.model_arg {
        parts.push(model_expr.map(|s| s.to_string()).unwrap_or_else(|| format!("PAzRefAny({})^", model)));
    }
    for p in &sig.params {
        parts.push(format!("{}({})^", p.ptr_type, p.name));
    }
    parts.join(", ")
}

/// Fill `out_ptr` with the kind's fallback value: the type's `_createDefault`
/// when it has one, else all-zero bytes (variant 0 of a fieldless enum).
fn emit_default_out(builder: &mut CodeBuilder, ret: &CallbackReturn) {
    match &ret.default_ctor {
        Some(c) => builder.line(&format!("if out_ptr <> nil then {}(out_ptr)^ := {}();", ret.ptr, c)),
        None => builder.line(&format!("if out_ptr <> nil then {}(out_ptr)^ := Default({});", ret.ptr, ret.raw)),
    }
}

/// Move the user's return value into `out_ptr` (wrapper classes release
/// their record and are freed; a nil wrapper yields the fallback value).
fn emit_write_out(builder: &mut CodeBuilder, ret: &CallbackReturn) {
    if ret.wrapper.is_some() {
        builder.line("if r = nil then");
        builder.line("begin");
        builder.indent();
        emit_default_out(builder, ret);
        builder.dedent();
        builder.line("end");
        builder.line("else");
        builder.line("begin");
        builder.line(&format!("  if out_ptr <> nil then {}(out_ptr)^ := r.Release;", ret.ptr));
        builder.line("  r.Free;");
        builder.line("end;");
    } else {
        builder.line(&format!("if out_ptr <> nil then {}(out_ptr)^ := r;", ret.ptr));
    }
}

/// Emit the `initialization` / `finalization` blocks at the very end of the
/// unit.
///
/// Besides registering the invoker stubs, the block masks every FPU
/// exception. libazul is Rust and C code compiled for the IEEE-754 DEFAULT
/// environment: NaN and ±inf are ordinary values in its arithmetic (taffy's
/// layout cache compares `abs(inf - inf)`, ratio code divides 0/0, ...). The
/// Free Pascal runtime instead UNMASKS InvalidOp, ZeroDivide and Overflow in
/// the FPU control register at program start, so the first such operation
/// inside the library trapped — and on aarch64-darwin the kernel delivers a
/// trapped FP exception as SIGILL, which the FPC RTL then reports as
/// "EAccessViolation: Access violation" with nothing wrong in memory. Masking
/// is what every FFI host of a C library has to do (Delphi's Set8087CW before
/// OpenGL/DirectX is the same rule); it costs nothing for Pascal code that
/// does not rely on FP traps.
///
/// It also self-checks the ABI (`AzulAbiCheck`): every fieldless enum that
/// crosses a host-invoker signature must be 4 bytes, i.e. `{$PACKENUM 4}`
/// must be in effect. A drift would otherwise surface as an EBusError deep
/// inside the engine (that is exactly how H1 of the review manifested).
pub fn emit_managed_initialization(builder: &mut CodeBuilder, ir: &CodegenIR) {
    builder.blank();
    builder.line("initialization");
    builder.indent();
    builder.line("{ libazul computes with NaN/inf as VALUES (IEEE default); the FPC RTL");
    builder.line("  unmasks InvalidOp/ZeroDivide/Overflow, which made the first inf - inf");
    builder.line("  inside the library trap as SIGILL -> \"EAccessViolation\". Mask them,");
    builder.line("  as every host of a C library must. Re-enable around your own code");
    builder.line("  only if you never call back into azul while they are unmasked. }");
    builder.line("SetExceptionMask([exInvalidOp, exDenormalized, exZeroDivide, exOverflow,");
    builder.line("                  exUnderflow, exPrecision]);");
    builder.line("AzulAbiCheck;");
    builder.line("InitCriticalSection(AzulHandlesLock);");
    builder.line("AzulHostInvokerInit;");
    builder.dedent();
    builder.blank();
    builder.line("finalization");
    builder.indent();
    builder.line("DoneCriticalSection(AzulHandlesLock);");
    builder.dedent();
    builder.blank();
}
