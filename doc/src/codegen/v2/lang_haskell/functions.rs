//! Raw `foreign import ccall` declarations for the C-ABI symbols.
//!
//! Every IR `FunctionDef` that survives the inclusion filter becomes a
//! single Haskell binding of the shape:
//!
//! ```haskell
//! foreign import ccall safe "AzApp_create_via"
//!   c_AzApp_create_via :: Ptr RefAny -> Ptr AppConfig -> Ptr App -> IO ()
//! ```
//!
//! Conventions:
//! - The Haskell-side identifier is `c_<C symbol>` so the FFI bindings
//!   are textually distinct from the idiomatic surface.
//! - Every import is `safe`. A `RefAny` built by `refAnyCreate` carries a
//!   host handle whose destructor calls back into Haskell through the
//!   registered releaser, so ANY function that may drop a `RefAny` — every
//!   `_delete`, every by-value consumer — can re-enter Haskell. A call-in
//!   during an `unsafe` foreign call is undefined behaviour in GHC
//!   (deadlock or abort); the cost of `safe` is a few nanoseconds per call
//!   against a GUI toolkit's frame budget.
//! - Every C function is treated as living in `IO`, since calls have
//!   side effects from Haskell's perspective even when the Rust side
//!   is morally pure (e.g. construction of a `Dom`).
//! - Argument and return types use the Haskell representation chosen
//!   in `types.rs` for the matching IR type. Pointers to FFI types
//!   become `Ptr <Name>`; primitives become their `Foreign.C.Types`
//!   equivalent.
//! - Functions whose C-ABI signature passes or returns a struct by value
//!   route through the `<name>_via` shim (`cshim.rs`): aggregate args are
//!   `Ptr T`, an aggregate return is a trailing `Ptr T` out-parameter.

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{ArgRefKind, CodegenIR, FieldRefKind, FunctionDef, TypeCategory};
use super::super::managed_host_invoker;
use super::sanitize_doc;
use super::types::haskell_field_type;

// ============================================================================
// Top-level entry
// ============================================================================

pub fn emit_foreign_imports(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    for func in &ir.functions {
        if !should_emit_function(func, ir, config) {
            continue;
        }
        emit_one(builder, func, ir);
    }

    emit_host_invoker_imports(builder, ir, config);

    // Callback wrappers: emit `foreign import ccall "wrapper"` for each
    // callback typedef so users can pass Haskell functions across the
    // FFI as `FunPtr`s.
    builder.blank();
    builder.line("-- ---------------------------------------------------------------------------");
    builder.line("-- Callback wrappers: turn a Haskell function into a C function pointer.");
    builder.line("-- ---------------------------------------------------------------------------");
    builder.blank();
    for cb in &ir.callback_typedefs {
        if !config.should_include_type(&cb.name) {
            continue;
        }
        emit_callback_wrapper(builder, cb, ir);
    }

    // Inbound trampolines: GHC's `foreign import ccall "wrapper"` can't
    // match the C ABI's by-value-struct-arg / by-value-struct-return
    // shape for callbacks. For each callback typedef, the cshim emits a
    // C trampoline that handles the ABI shape and forwards to a
    // Haskell-friendly inner with by-pointer args and out-pointer
    // return. The three imports below let user code:
    //
    //   1. Wrap a Haskell fn `(Ptr Arg1 -> ... -> Ptr Ret -> IO ())` as
    //      a `FunPtr` via `mk_<X>_inner`.
    //   2. Register that FunPtr via `c_<X>_set_inner` so the trampoline
    //      knows where to delegate.
    //   3. Take `p_<X>_trampoline` as the actual C fn pointer to splice
    //      into AzLayoutCallback / button.with_on_click / etc.
    //
    // This is the raw path for callback kinds WITHOUT a host invoker; the
    // kinds in `HOST_INVOKER_KINDS` go through `Azul`'s managed layer.
    builder.blank();
    builder.line("-- ---------------------------------------------------------------------------");
    builder.line("-- Inbound trampolines (Haskell-friendly out-pointer inner + C-ABI trampoline).");
    builder.line("-- See cbits/azul_shims.c for the matching `Az<X>_trampoline` / `_set_inner`.");
    builder.line("-- ---------------------------------------------------------------------------");
    builder.blank();
    for cb in &ir.callback_typedefs {
        if !config.should_include_type(&cb.name) {
            continue;
        }
        emit_inbound_trampoline_imports(builder, cb, ir);
    }
    Ok(())
}

/// Imports for the host-invoker protocol (`core/src/host_invoker.rs`): the
/// shared handle releaser, host-handle `RefAny` constructors, and per
/// callback kind the invoker setter + the `createFromHostHandle` factory.
/// The `_via` forms are the shims `cshim.rs` emits for the by-value
/// returns. The managed layer in `Azul` builds `refAnyCreate` and the
/// closure-taking callback setters on top of these.
fn emit_host_invoker_imports(builder: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    builder.blank();
    builder.line("-- ---------------------------------------------------------------------------");
    builder.line("-- Host-invoker protocol: host-handle RefAny + per-kind invokers.");
    builder.line("-- ---------------------------------------------------------------------------");
    builder.blank();
    builder.line("foreign import ccall \"wrapper\"");
    builder.indent();
    builder.line("mk_HostHandleReleaser :: (Word64 -> IO ()) -> IO (FunPtr (Word64 -> IO ()))");
    builder.dedent();
    builder.line("foreign import ccall safe \"AzApp_setHostHandleReleaser\"");
    builder.indent();
    builder.line("c_AzApp_setHostHandleReleaser :: FunPtr (Word64 -> IO ()) -> IO ()");
    builder.dedent();
    builder.line("foreign import ccall safe \"AzRefAny_newHostHandle_via\"");
    builder.indent();
    builder.line("c_AzRefAny_newHostHandle_via :: Word64 -> Ptr RefAny -> IO ()");
    builder.dedent();
    builder.line("foreign import ccall safe \"AzRefAny_getHostHandle\"");
    builder.indent();
    builder.line("c_AzRefAny_getHostHandle :: Ptr RefAny -> IO Word64");
    builder.dedent();
    builder.blank();

    for cb in managed_host_invoker::host_invoker_kinds(ir) {
        if !config.should_include_type(&cb.name) {
            continue;
        }
        let wrapper = managed_host_invoker::wrapper_name(cb);
        if ir.find_struct(wrapper).is_none() {
            continue;
        }
        let sig = host_invoker_signature(cb, ir);
        builder.line(&format!("-- {} invoker: handle, one pointer per callback arg, out-pointer return.", wrapper));
        builder.line("foreign import ccall \"wrapper\"");
        builder.indent();
        builder.line(&format!(
            "mk_{}Invoker :: ({}) -> IO (FunPtr ({}))",
            wrapper, sig, sig
        ));
        builder.dedent();
        builder.line(&format!(
            "foreign import ccall safe \"AzApp_set{}Invoker\"",
            wrapper
        ));
        builder.indent();
        builder.line(&format!(
            "c_AzApp_set{}Invoker :: FunPtr ({}) -> IO ()",
            wrapper, sig
        ));
        builder.dedent();
        builder.line(&format!(
            "foreign import ccall safe \"Az{}_createFromHostHandle_via\"",
            wrapper
        ));
        builder.indent();
        builder.line(&format!(
            "c_Az{}_createFromHostHandle_via :: Word64 -> Ptr {} -> IO ()",
            wrapper,
            super::haskell_data_name(wrapper)
        ));
        builder.dedent();
        builder.blank();
    }
}

/// The Haskell type of a host invoker for `cb`, mirroring
/// `managed_host_invoker::invoker_c_arg_list`: `Word64 -> Ptr A1 -> ... ->
/// Ptr R -> IO ()` (the trailing out-pointer only when the callback
/// returns a value).
pub(super) fn host_invoker_signature(
    cb: &super::super::ir::CallbackTypedefDef,
    ir: &CodegenIR,
) -> String {
    let mut parts = vec!["Word64".to_string()];
    for a in &cb.args {
        let raw = haskell_field_type(&a.type_name, FieldRefKind::Owned, ir);
        parts.push(format!("Ptr {}", paren_if_needed(&raw)));
    }
    if managed_host_invoker::has_return(cb) {
        let raw = haskell_field_type(
            cb.return_type.as_deref().unwrap_or("()"),
            FieldRefKind::Owned,
            ir,
        );
        parts.push(format!("Ptr {}", paren_if_needed(&raw)));
    }
    format!("{} -> IO ()", parts.join(" -> "))
}

/// Per-callback-typedef `register<X>Callback` helpers that hide the
/// inbound-trampoline triplet (mk_inner + set_inner + trampoline) behind a
/// single user-facing API. The user passes a Haskell function of the
/// natural shape (`Ptr Arg1 -> ... -> IO Ret`); the helper wraps it,
/// registers it as the inner, and returns `FunPtr ()` to splice into
/// libazul's C-ABI parameter. One static slot per kind — the newest
/// registration wins for the whole kind.
pub fn emit_callback_register_helpers(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    if ir.callback_typedefs.is_empty() {
        return Ok(());
    }
    builder.blank();
    builder.line("-- ---------------------------------------------------------------------------");
    builder.line("-- Per-callback-typedef `register<X>Callback` helpers (raw trampoline path).");
    builder.line("-- ---------------------------------------------------------------------------");
    builder.blank();
    for cb in &ir.callback_typedefs {
        if !config.should_include_type(&cb.name) {
            continue;
        }
        emit_one_register_helper(builder, cb, ir);
    }
    Ok(())
}

fn emit_one_register_helper(
    builder: &mut CodeBuilder,
    cb: &super::super::ir::CallbackTypedefDef,
    ir: &CodegenIR,
) {
    // Build user-facing arg signature, mirroring exactly what
    // `mk_<X>_inner` accepts (minus the trailing out-ptr for aggregate
    // returns).
    let mut user_arg_types: Vec<String> = Vec::new();
    for a in &cb.args {
        let kind = match a.ref_kind {
            ArgRefKind::Owned => FieldRefKind::Owned,
            ArgRefKind::Ref => FieldRefKind::Ref,
            ArgRefKind::RefMut => FieldRefKind::RefMut,
            ArgRefKind::Ptr => FieldRefKind::Ptr,
            ArgRefKind::PtrMut => FieldRefKind::PtrMut,
        };
        let raw = haskell_field_type(&a.type_name, kind, ir);
        if is_haskell_ffi_primitive(&raw) {
            user_arg_types.push(raw);
        } else {
            user_arg_types.push(format!("Ptr {}", paren_if_needed(&raw)));
        }
    }

    // Return-type classification (matches inbound trampoline emission).
    let returns_void = match cb.return_type.as_deref() {
        None => true,
        Some(r) => matches!(r.trim(), "" | "void" | "()" | "c_void"),
    };
    let ret_raw = match cb.return_type.as_deref() {
        None => "()".to_string(),
        Some(r) => {
            let t = r.trim();
            if matches!(t, "" | "void" | "()" | "c_void") {
                "()".to_string()
            } else {
                haskell_field_type(t, FieldRefKind::Owned, ir)
            }
        }
    };
    let ret_is_aggregate = !returns_void && !is_haskell_ffi_primitive(&ret_raw);

    let user_func_ty = if user_arg_types.is_empty() {
        format!("IO {}", paren_if_needed(&ret_raw))
    } else {
        format!(
            "{} -> IO {}",
            user_arg_types
                .iter()
                .map(|a| paren_if_needed(a))
                .collect::<Vec<_>>()
                .join(" -> "),
            paren_if_needed(&ret_raw)
        )
    };

    let helper_name = format!("register{}Callback", cb.name);

    builder.line(&format!(
        "-- | Register a user function as the {} callback.",
        cb.name
    ));
    builder.line("--");
    builder.line("-- The returned 'FunPtr ()' is the C function pointer to splice into");
    builder.line("-- the libazul parameter that takes this callback typedef.");
    builder.line(&format!(
        "{} :: ({}) -> IO (FunPtr ())",
        helper_name, user_func_ty
    ));
    builder.line(&format!("{} userFn = do", helper_name));
    builder.indent();
    let args_vars: Vec<String> = (0..cb.args.len()).map(|i| format!("a{}", i)).collect();
    let call_args = args_vars.join(" ");
    if ret_is_aggregate {
        let args_pat = if args_vars.is_empty() {
            String::from("outPtr")
        } else {
            format!("{} outPtr", args_vars.join(" "))
        };
        builder.line(&format!(
            "innerFn <- mk_{}_inner $ \\{} -> do",
            cb.name, args_pat,
        ));
        builder.indent();
        builder.line(&format!("__ret <- userFn {}", call_args));
        builder.line("poke outPtr __ret");
        builder.dedent();
    } else {
        let args_pat = args_vars.join(" ");
        builder.line(&format!(
            "innerFn <- mk_{}_inner $ \\{} -> userFn {}",
            cb.name, args_pat, call_args,
        ));
    }
    builder.line(&format!("c_Az{}_set_inner innerFn", cb.name));
    builder.line(&format!("pure p_Az{}_trampoline", cb.name));
    builder.dedent();
    builder.blank();
}

/// The ONE inclusion predicate for this binding. `cshim::should_emit_shim_for`
/// is defined as `should_emit_function(..) && needs_shim(..)`, so a function
/// can never get a `foreign import "<name>_via"` without the C shim that
/// defines `<name>_via`.
pub(super) fn should_emit_function(func: &FunctionDef, ir: &CodegenIR, config: &CodegenConfig) -> bool {
    // A trait entry point an api.json `derive` declares is not what the
    // `DestructorOrClone` exclusion below is for. That category is excluded
    // because those types' ordinary methods traffic in callback function
    // pointers this binding cannot marshal; `Az{T}_toDbgString(ptr) ->
    // AzString` traffics in neither, and is the same shape as the ~2800
    // `_toDbgString` declarations this binding already emits. RECURSIVE types
    // are here for the same reason.
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
        ) {
            return false;
        }
        if !s.generic_params.is_empty() {
            return false;
        }
    }
    if let Some(e) = ir.find_enum(&func.class_name) {
        if matches!(
            e.category,
            TypeCategory::Recursive
                | TypeCategory::DestructorOrClone
                | TypeCategory::GenericTemplate
        ) {
            return false;
        }
        if !e.generic_params.is_empty() {
            return false;
        }
    }
    true
}

// ============================================================================
// FFI signature (shared with the wrapper layer)
// ============================================================================

/// The Haskell-side shape of one C-ABI import. `Azul` (the wrapper layer)
/// builds its call sites from this so the two can never disagree about
/// which arguments travel by pointer.
pub(super) struct FfiSig {
    /// `c_<symbol>` or `c_<symbol>_via`.
    pub binding: String,
    /// True when the import points at the `_via` shim (aggregate args are
    /// `Ptr T`, an aggregate return is a trailing out-pointer).
    pub shimmed: bool,
    /// Haskell type of each C argument, in order (without the out-pointer).
    pub arg_types: Vec<String>,
    /// Haskell type of the trailing out-pointer's pointee for aggregate
    /// returns through the shim.
    pub out_type: Option<String>,
    /// Haskell return type (`()` when void or returned through `out_type`).
    pub ret_type: String,
}

impl FfiSig {
    pub fn haskell_type(&self) -> String {
        let mut atoms: Vec<String> = self.arg_types.iter().map(|a| paren_if_needed(a)).collect();
        if let Some(out) = &self.out_type {
            atoms.push(format!("Ptr {}", paren_if_needed(out)));
        }
        if atoms.is_empty() {
            format!("IO {}", paren_if_needed(&self.ret_type))
        } else {
            format!("{} -> IO {}", atoms.join(" -> "), paren_if_needed(&self.ret_type))
        }
    }
}

pub(super) fn ffi_signature(func: &FunctionDef, ir: &CodegenIR) -> FfiSig {
    let shimmed = super::cshim::needs_shim(func);
    let arg_types = build_haskell_args(func, ir);
    let aggregate_return = shimmed && super::cshim::return_is_aggregate(func);
    let (out_type, ret_type) = if aggregate_return {
        let r = func.return_type.as_deref().unwrap();
        (Some(map_arg_owned(r, ir)), "()".to_string())
    } else {
        (None, build_haskell_return(func, ir))
    };
    FfiSig {
        binding: if shimmed {
            format!("c_{}_via", func.c_name)
        } else {
            format!("c_{}", func.c_name)
        },
        shimmed,
        arg_types,
        out_type,
        ret_type,
    }
}

// ============================================================================
// Function emission
// ============================================================================

fn emit_one(builder: &mut CodeBuilder, func: &FunctionDef, ir: &CodegenIR) {
    if !func.doc.is_empty() {
        for d in &func.doc {
            builder.line(&format!("-- | {}", sanitize_doc(d)));
        }
    }
    let sig = ffi_signature(func, ir);
    let symbol = if sig.shimmed {
        format!("{}_via", func.c_name)
    } else {
        func.c_name.clone()
    };
    builder.line(&format!("foreign import ccall safe \"{}\"", symbol));
    builder.indent();
    builder.line(&format!("{} :: {}", sig.binding, sig.haskell_type()));
    builder.dedent();
}

fn build_haskell_args(func: &FunctionDef, ir: &CodegenIR) -> Vec<String> {
    let mut atoms: Vec<String> = Vec::new();
    for a in &func.args {
        let ty = match a.ref_kind {
            ArgRefKind::Owned => map_arg_owned_ffi(&a.type_name, ir),
            ArgRefKind::Ref | ArgRefKind::RefMut | ArgRefKind::Ptr | ArgRefKind::PtrMut => {
                format!("Ptr {}", paren_if_needed(&map_arg_owned(&a.type_name, ir)))
            }
        };
        atoms.push(ty);
    }
    atoms
}

fn build_haskell_return(func: &FunctionDef, ir: &CodegenIR) -> String {
    let returns_void = func
        .return_type
        .as_ref()
        .map(|r| {
            let t = r.trim();
            matches!(t, "" | "void" | "()" | "c_void")
        })
        .unwrap_or(true);
    if returns_void {
        "()".to_string()
    } else {
        ffi_return_type(func.return_type.as_deref().unwrap_or("()"), ir)
    }
}

/// A C return value: primitives by value, aggregates through the shim's
/// out-pointer (handled by the caller), and pointer-typed returns
/// (`*const u8` from `AzImageRef_getBytesPtr`) as the pointer itself. Unlike
/// an argument — where a pointer-spelled type name travels through the shim
/// as a pointer to the pointer — a returned pointer is what the C function
/// returns.
fn ffi_return_type(type_name: &str, ir: &CodegenIR) -> String {
    let raw = haskell_field_type(type_name, FieldRefKind::Owned, ir);
    if raw.starts_with("(Ptr ") {
        raw
    } else {
        map_arg_owned_ffi(type_name, ir)
    }
}

// ============================================================================
// Callback wrappers (`foreign import ccall "wrapper"`)
// ============================================================================

fn emit_callback_wrapper(
    builder: &mut CodeBuilder,
    cb: &super::super::ir::CallbackTypedefDef,
    ir: &CodegenIR,
) {
    if !cb.doc.is_empty() {
        for d in &cb.doc {
            builder.line(&format!("-- | {}", sanitize_doc(d)));
        }
    }

    // Build the Haskell function type for the callback. Same `Ptr T`
    // wrapping for aggregates as the regular foreign-import emit
    // (GHC's "wrapper" import inherits the same FFI restrictions).
    let mut atoms: Vec<String> = Vec::new();
    for a in &cb.args {
        let ty = match a.ref_kind {
            ArgRefKind::Owned => map_arg_owned_ffi(&a.type_name, ir),
            ArgRefKind::Ref | ArgRefKind::RefMut | ArgRefKind::Ptr | ArgRefKind::PtrMut => {
                format!("Ptr {}", paren_if_needed(&map_arg_owned(&a.type_name, ir)))
            }
        };
        atoms.push(ty);
    }

    let returns_void = cb
        .return_type
        .as_ref()
        .map(|r| {
            let t = r.trim();
            matches!(t, "" | "void" | "()" | "c_void")
        })
        .unwrap_or(true);
    let ret_ty = if returns_void {
        "()".to_string()
    } else {
        ffi_return_type(cb.return_type.as_deref().unwrap_or("()"), ir)
    };

    let func_ty = if atoms.is_empty() {
        format!("IO {}", paren_if_needed(&ret_ty))
    } else {
        format!(
            "{} -> IO {}",
            atoms
                .iter()
                .map(|a| paren_if_needed(a))
                .collect::<Vec<_>>()
                .join(" -> "),
            paren_if_needed(&ret_ty)
        )
    };

    let mk_name = format!("mk_{}", cb.name);
    builder.line("foreign import ccall \"wrapper\"");
    builder.indent();
    builder.line(&format!(
        "{} :: ({}) -> IO (FunPtr ({}))",
        mk_name, func_ty, func_ty
    ));
    builder.dedent();
}

/// Emit the inbound-trampoline import triplet for one callback typedef:
///
/// ```haskell
/// foreign import ccall "wrapper"
///     mk_LayoutCallbackType_inner :: (Ptr RefAny -> Ptr LayoutCallbackInfo -> Ptr Dom -> IO ())
///                                 -> IO (FunPtr (...))
/// foreign import ccall "AzLayoutCallbackType_set_inner"
///     c_AzLayoutCallbackType_set_inner :: FunPtr (...) -> IO ()
/// foreign import ccall "&AzLayoutCallbackType_trampoline"
///     p_AzLayoutCallbackType_trampoline :: FunPtr ()
/// ```
///
/// The wrapper inner signature uses by-pointer args + out-pointer return
/// so GHC's `foreign import ccall "wrapper"` accepts it; the matching C
/// trampoline (see `cshim.rs::emit_inbound_trampoline`) dereferences /
/// addresses-of as needed to bridge with the C ABI's by-value-struct
/// shape that libazul invokes.
fn emit_inbound_trampoline_imports(
    builder: &mut CodeBuilder,
    cb: &super::super::ir::CallbackTypedefDef,
    ir: &CodegenIR,
) {
    // Inner signature: each arg is `Ptr T` (Haskell can't take aggregate
    // structs by value); aggregate return becomes a trailing `Ptr R`
    // out-parameter; primitive return stays as the primitive.
    let mut inner_atoms: Vec<String> = Vec::new();
    for a in &cb.args {
        let raw = haskell_field_type(&a.type_name, FieldRefKind::Owned, ir);
        if is_haskell_ffi_primitive(&raw) {
            inner_atoms.push(raw);
        } else {
            inner_atoms.push(format!("Ptr {}", paren_if_needed(&raw)));
        }
    }

    let ret_is_aggregate = match cb.return_type.as_deref() {
        Some(r) => {
            let t = r.trim();
            if matches!(t, "" | "void" | "()" | "c_void") {
                false
            } else {
                let raw = haskell_field_type(t, FieldRefKind::Owned, ir);
                !is_haskell_ffi_primitive(&raw)
            }
        }
        None => false,
    };

    let inner_ret_ty;
    if ret_is_aggregate {
        let raw = haskell_field_type(cb.return_type.as_deref().unwrap(), FieldRefKind::Owned, ir);
        inner_atoms.push(format!("Ptr {}", paren_if_needed(&raw)));
        inner_ret_ty = "()".to_string();
    } else {
        inner_ret_ty = match cb.return_type.as_deref() {
            None => "()".to_string(),
            Some(r) => {
                let t = r.trim();
                if matches!(t, "" | "void" | "()" | "c_void") {
                    "()".to_string()
                } else {
                    haskell_field_type(t, FieldRefKind::Owned, ir)
                }
            }
        };
    }

    let inner_func_ty = if inner_atoms.is_empty() {
        format!("IO {}", paren_if_needed(&inner_ret_ty))
    } else {
        format!(
            "{} -> IO {}",
            inner_atoms
                .iter()
                .map(|a| paren_if_needed(a))
                .collect::<Vec<_>>()
                .join(" -> "),
            paren_if_needed(&inner_ret_ty)
        )
    };

    let mk_inner_name = format!("mk_{}_inner", cb.name);
    let c_setter_name = format!("c_Az{}_set_inner", cb.name);
    let p_trampoline_name = format!("p_Az{}_trampoline", cb.name);

    builder.line("foreign import ccall \"wrapper\"");
    builder.indent();
    builder.line(&format!(
        "{} :: ({}) -> IO (FunPtr ({}))",
        mk_inner_name, inner_func_ty, inner_func_ty
    ));
    builder.dedent();

    builder.line(&format!(
        "foreign import ccall safe \"Az{}_set_inner\"",
        cb.name
    ));
    builder.indent();
    builder.line(&format!(
        "{} :: FunPtr ({}) -> IO ()",
        c_setter_name, inner_func_ty
    ));
    builder.dedent();

    // p_<X>_trampoline is the C fn-pointer value of the trampoline,
    // imported as a FunPtr to splice into AzLayoutCallback / Button.
    // Haskell never *calls* through it; it only needs to be the right
    // size to poke into a struct field, hence `FunPtr ()`.
    builder.line(&format!(
        "foreign import ccall unsafe \"&Az{}_trampoline\"",
        cb.name
    ));
    builder.indent();
    builder.line(&format!("{} :: FunPtr ()", p_trampoline_name));
    builder.dedent();
    builder.blank();
}

// ============================================================================
// Helpers
// ============================================================================

/// Map an argument's IR type (without ref-kind decoration) to the
/// matching Haskell type. We share with `types::haskell_field_type`
/// for the leaf-type mapping by faking an Owned ref-kind.
pub(super) fn map_arg_owned(type_name: &str, ir: &CodegenIR) -> String {
    haskell_field_type(type_name, FieldRefKind::Owned, ir)
}

/// Map a type as an FFI argument/return value. GHC's foreign-import
/// allows pass-by-value for primitives only — any aggregate type must
/// be wrapped in `Ptr T`. This wrapper does that automatically so the
/// generated `foreign import ccall` declarations type-check.
fn map_arg_owned_ffi(type_name: &str, ir: &CodegenIR) -> String {
    let raw = haskell_field_type(type_name, FieldRefKind::Owned, ir);
    if is_haskell_ffi_primitive(&raw) {
        raw
    } else {
        // Wrap aggregates in `Ptr T` so the C ABI's by-value struct
        // becomes a pointer-to-struct at the Haskell FFI boundary.
        // Caller-side marshalling (alloca/poke/peek) happens in the
        // wrapper layer.
        format!("Ptr {}", paren_if_needed(&raw))
    }
}

/// Haskell primitive types that GHC's foreign-import allows by value.
pub(super) fn is_haskell_ffi_primitive(ty: &str) -> bool {
    matches!(
        ty,
        "()" | "Int"
            | "Word"
            | "Int8"
            | "Int16"
            | "Int32"
            | "Int64"
            | "Word8"
            | "Word16"
            | "Word32"
            | "Word64"
            | "Char"
            | "CBool"
            | "CChar"
            | "CSChar"
            | "CUChar"
            | "CShort"
            | "CUShort"
            | "CInt"
            | "CUInt"
            | "CLong"
            | "CULong"
            | "CLLong"
            | "CULLong"
            | "CFloat"
            | "CDouble"
            | "CSize"
            | "CSSize"
            | "CIntPtr"
            | "CUIntPtr"
            | "CIntMax"
            | "CUIntMax"
            | "CPtrdiff"
            | "CWchar"
    ) || ty.starts_with("Ptr ")
        || ty.starts_with("FunPtr ")
}

/// Wrap a multi-token type expression in parens so the surrounding
/// signature parses unambiguously (`Ptr Foo` would otherwise bind
/// `Ptr` only).
pub(super) fn paren_if_needed(s: &str) -> String {
    let needs = s.contains(' ') && !(s.starts_with('(') && s.ends_with(')'));
    if needs {
        format!("({})", s)
    } else {
        s.to_string()
    }
}
