//! C shim layer for Haskell bindings.
//!
//! GHC's foreign-import doesn't support passing or returning C structs
//! by value. Every C-ABI function with an aggregate arg or return
//! therefore needs a wrapper that:
//!
//! - Takes by-value aggregate args as `const T *` (Haskell allocates +
//!   pokes; the shim dereferences before calling).
//! - Takes by-value aggregate returns as a trailing `T *az_out`
//!   (Haskell allocates; the shim writes the return through it; the
//!   foreign-import returns `void`).
//!
//! Pointer args and primitive args / returns pass through unchanged.
//!
//! The shims are emitted into `cbits/azul_shims.c` and compiled into
//! the cabal library via the `c-sources` field. Foreign-imports point
//! at the `<C symbol>_via` names; the Haskell wrapper layer in
//! `Internal.FFI` hides the alloca dance so user code keeps the
//! natural `args -> IO T` shape.

use super::super::ir::{ArgRefKind, CallbackTypedefDef, CodegenIR, FunctionDef, TypeCategory};
use super::super::config::CodegenConfig;

/// Top-level entry: produce the full `cbits/azul_shims.c` source as a
/// single string, including the necessary `#include`s.
pub fn generate_c_shims(ir: &CodegenIR, config: &CodegenConfig) -> String {
    let mut out = String::with_capacity(64 * 1024);
    out.push_str(
        "/* ============================================================ */\n\
         /* Auto-generated C shims for the Haskell Azul bindings.        */\n\
         /* GHC's FFI doesn't support struct-by-value across the         */\n\
         /* boundary; every function whose C signature uses one gets a   */\n\
         /* `<name>_via` wrapper that takes/returns through pointers.   */\n\
         /* ============================================================ */\n\n\
         #include \"azul.h\"\n\n",
    );
    for func in &ir.functions {
        if !should_emit_shim_for(func, ir, config) {
            continue;
        }
        emit_one(&mut out, func, ir);
    }

    // Inbound trampolines: per callback typedef, emit a C function that
    // matches the C ABI's by-value-struct signature and forwards to a
    // Haskell-friendly inner function pointer (out-pointer return). GHC's
    // `foreign import ccall "wrapper"` cannot match the by-value C ABI
    // directly, so libazul calls the trampoline; the trampoline
    // dereferences/addresses-of as needed and delegates to the inner.
    out.push_str(
        "\n\
         /* ============================================================ */\n\
         /* Inbound trampolines for callback typedefs.                   */\n\
         /* Bridges the C ABI's by-value-struct signature to a Haskell-  */\n\
         /* friendly out-pointer signature. One static slot per typedef  */\n\
         /* — registers a Haskell-generated FunPtr via `_set_inner`,     */\n\
         /* and exports `_trampoline` as the C function pointer to       */\n\
         /* splice into AzLayoutCallback / button.with_on_click / etc.   */\n\
         /* ============================================================ */\n\n",
    );
    for cb in &ir.callback_typedefs {
        if !should_emit_inbound_trampoline(cb, config) {
            continue;
        }
        emit_inbound_trampoline(&mut out, cb);
    }
    out
}

/// True if a callback typedef needs an inbound trampoline. We emit one
/// for every included callback typedef — even primitive-only ones — so
/// the user-facing API is uniform (always go through `_inner` + setter +
/// trampoline). The cost is one extra indirection per call.
fn should_emit_inbound_trampoline(cb: &CallbackTypedefDef, config: &CodegenConfig) -> bool {
    config.should_include_type(&cb.name)
}

/// Emit the inbound-trampoline triplet for one callback typedef:
///
/// ```c
/// /* Inner sig (Haskell-friendly: by-pointer args, out-ptr return). */
/// typedef void (*AzN_inner)(<ptr-args>, AzR *az_out);
/// static AzN_inner g_AzN_inner = 0;
/// void AzN_set_inner(AzN_inner f) { g_AzN_inner = f; }
/// /* Trampoline matches the C-ABI by-value signature. */
/// AzR AzN_trampoline(<by-value-args>) {
///     AzR __ret;
///     g_AzN_inner(<address-of-args>, &__ret);
///     return __ret;
/// }
/// ```
///
/// For void-returning callbacks the trampoline omits the `__ret` plumbing.
/// For primitive-returning callbacks the inner takes the primitive return
/// by value (no out-pointer).
fn emit_inbound_trampoline(out: &mut String, cb: &CallbackTypedefDef) {
    let returns_void = match &cb.return_type {
        None => true,
        Some(r) => matches!(r.trim(), "" | "void" | "()" | "c_void"),
    };
    let ret_is_aggregate = match &cb.return_type {
        Some(r) => {
            let t = r.trim();
            !matches!(t, "" | "void" | "()" | "c_void")
                && !t.starts_with("*const ")
                && !t.starts_with("*mut ")
                && !t.starts_with('&')
                && !is_c_primitive(t)
        }
        None => false,
    };

    let inner_name = format!("Az{}_inner", cb.name);
    let setter_name = format!("Az{}_set_inner", cb.name);
    let trampoline_name = format!("Az{}_trampoline", cb.name);

    // Build the by-value (C-ABI) parameter list and the
    // address-of-passing inner-call argument list.
    let mut abi_params: Vec<String> = Vec::new();
    let mut inner_args: Vec<String> = Vec::new();
    let mut inner_params: Vec<String> = Vec::new();
    for (idx, a) in cb.args.iter().enumerate() {
        let raw_name = if a.name.is_empty() {
            format!("_arg{}", idx)
        } else {
            sanitize_c_arg(&a.name)
        };
        // Type names like `*mut T` / `*const T` represent raw-pointer
        // args; c_typename already inlines the `*` so we treat them as
        // pointers regardless of the surrounding ref_kind (which the IR
        // sometimes records as Owned for these encodings).
        let type_is_ptr_prefix = a.type_name.starts_with("*mut ")
            || a.type_name.starts_with("*const ");
        let c_ty = c_typename(&a.type_name);
        if type_is_ptr_prefix {
            // c_ty already ends in ` *` (or `const T *`) — emit the
            // identifier directly. No extra address-of needed when
            // forwarding to the inner.
            abi_params.push(format!("{}{}", c_ty, raw_name));
            inner_args.push(raw_name.clone());
            inner_params.push(format!("{}{}", c_ty, raw_name));
            continue;
        }
        match a.ref_kind {
            ArgRefKind::Owned => {
                if is_c_primitive(&a.type_name) {
                    // Primitive args pass by value end-to-end.
                    abi_params.push(format!("{} {}", c_ty, raw_name));
                    inner_args.push(raw_name.clone());
                    inner_params.push(format!("{} {}", c_ty, raw_name));
                } else {
                    // Aggregate by-value at the C ABI; Haskell can't
                    // take it by value, so the trampoline takes the
                    // address of its local copy and passes a pointer
                    // through.
                    abi_params.push(format!("{} {}", c_ty, raw_name));
                    inner_args.push(format!("&{}", raw_name));
                    inner_params.push(format!("const {} *{}", c_ty, raw_name));
                }
            }
            ArgRefKind::Ref | ArgRefKind::Ptr => {
                abi_params.push(format!("const {} *{}", c_ty, raw_name));
                inner_args.push(raw_name.clone());
                inner_params.push(format!("const {} *{}", c_ty, raw_name));
            }
            ArgRefKind::RefMut | ArgRefKind::PtrMut => {
                abi_params.push(format!("{} *{}", c_ty, raw_name));
                inner_args.push(raw_name.clone());
                inner_params.push(format!("{} *{}", c_ty, raw_name));
            }
        }
    }

    let abi_params_str = if abi_params.is_empty() {
        "void".to_string()
    } else {
        abi_params.join(", ")
    };

    // Inner-signature out-pointer return (aggregate case) or by-value
    // primitive return.
    let (inner_ret_c, abi_ret_c) = match cb.return_type.as_deref() {
        None => ("void".to_string(), "void".to_string()),
        Some(r) => {
            let t = r.trim();
            if matches!(t, "" | "void" | "()" | "c_void") {
                ("void".to_string(), "void".to_string())
            } else if ret_is_aggregate {
                // Inner signature gets a trailing `AzR *az_out` and
                // returns void; trampoline returns `AzR` by value.
                ("void".to_string(), c_typename(t))
            } else {
                // Primitive return: inner returns the primitive too.
                (c_typename(t), c_typename(t))
            }
        }
    };

    let mut inner_params_with_out = inner_params.clone();
    if ret_is_aggregate {
        inner_params_with_out.push(format!("{} *az_out", abi_ret_c));
    }
    let inner_params_str = if inner_params_with_out.is_empty() {
        "void".to_string()
    } else {
        inner_params_with_out.join(", ")
    };

    out.push_str(&format!(
        "/* === {} (return: {}, args: {}) === */\n",
        cb.name,
        cb.return_type.as_deref().unwrap_or("void"),
        cb.args.len()
    ));
    out.push_str(&format!(
        "typedef {} (*{})({});\n",
        inner_ret_c, inner_name, inner_params_str,
    ));
    out.push_str(&format!("static {} g_{} = 0;\n", inner_name, inner_name));
    out.push_str(&format!(
        "void {}({} f) {{ g_{} = f; }}\n",
        setter_name, inner_name, inner_name,
    ));

    // Trampoline body.
    let mut inner_args_with_out = inner_args.clone();
    if ret_is_aggregate {
        inner_args_with_out.push("&__ret".to_string());
    }
    let inner_call_args = inner_args_with_out.join(", ");

    if returns_void {
        out.push_str(&format!(
            "{} {}({}) {{ if (g_{}) g_{}({}); }}\n\n",
            abi_ret_c, trampoline_name, abi_params_str, inner_name, inner_name, inner_call_args,
        ));
    } else if ret_is_aggregate {
        out.push_str(&format!(
            "{} {}({}) {{ {} __ret; \
             if (g_{}) g_{}({}); \
             return __ret; }}\n\n",
            abi_ret_c,
            trampoline_name,
            abi_params_str,
            abi_ret_c,
            inner_name,
            inner_name,
            inner_call_args,
        ));
    } else {
        // Primitive return — inner returns the primitive directly.
        out.push_str(&format!(
            "{} {}({}) {{ return g_{} ? g_{}({}) : ({})0; }}\n\n",
            abi_ret_c,
            trampoline_name,
            abi_params_str,
            inner_name,
            inner_name,
            inner_call_args,
            abi_ret_c,
        ));
    }
}

/// True if a function passes the same inclusion filter as the
/// foreign-import emitter (so the shim's symbol resolves to the same
/// libazul export).
pub fn should_emit_shim_for(
    func: &FunctionDef,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> bool {
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
    // Only emit a shim when the function actually needs one — primitive-only
    // signatures pass through GHC's FFI natively.
    needs_shim(func)
}

/// Does this function's C-ABI signature have at least one struct-by-value
/// arg, or a struct-by-value return? Functions that are entirely
/// primitives + pointers + void can use GHC's foreign-import directly.
pub fn needs_shim(func: &FunctionDef) -> bool {
    if return_is_aggregate(func) {
        return true;
    }
    func.args.iter().any(|a| {
        matches!(a.ref_kind, ArgRefKind::Owned) && !is_c_primitive(&a.type_name)
    })
}

pub fn return_is_aggregate(func: &FunctionDef) -> bool {
    let Some(r) = func.return_type.as_deref() else {
        return false;
    };
    let t = r.trim();
    if matches!(t, "" | "void" | "()" | "c_void") {
        return false;
    }
    // Pointer-syntax returns (`*const T` / `*mut T`) aren't aggregates;
    // GHC's foreign-import handles them as `Ptr T` directly. Same for
    // reference syntax (which only appears at the IR's arg level but
    // be defensive).
    if t.starts_with("*const ") || t.starts_with("*mut ") || t.starts_with('&') {
        return false;
    }
    !is_c_primitive(t)
}

fn is_c_primitive(t: &str) -> bool {
    matches!(
        t.trim(),
        "u8" | "u16" | "u32" | "u64"
            | "i8" | "i16" | "i32" | "i64"
            | "usize" | "isize"
            | "f32" | "f64"
            | "bool" | "()"
            | "c_void" | "void"
            | "c_char" | "c_uchar" | "c_int" | "c_uint"
            | "c_long" | "c_ulong" | "c_longlong" | "c_ulonglong"
            | "size_t" | "ssize_t" | "intptr_t" | "uintptr_t"
            | "char"
    )
}

/// Map a Rust/IR type name to its C-ABI typename. Primitives go to
/// their `<stdint.h>` form; everything else gets the `Az` prefix that
/// the generated `azul.h` uses.
fn c_typename(t: &str) -> String {
    let t = t.trim();
    // Pointer-prefix forms: `*mut T` → `T *`, `*const T` → `const T *`.
    // The IR encodes some raw-pointer types this way (e.g.
    // `*mut c_void` for RefAnyDestructorType arg). Recurse into the
    // pointee so the C output picks up `void *` / `AzFoo *` rather than
    // the literal pasted `Az*mut c_void`.
    if let Some(inner) = t.strip_prefix("*mut ") {
        return format!("{} *", c_typename(inner));
    }
    if let Some(inner) = t.strip_prefix("*const ") {
        return format!("const {} *", c_typename(inner));
    }
    match t {
        "u8" => "uint8_t".to_string(),
        "u16" => "uint16_t".to_string(),
        "u32" => "uint32_t".to_string(),
        "u64" => "uint64_t".to_string(),
        "i8" => "int8_t".to_string(),
        "i16" => "int16_t".to_string(),
        "i32" => "int32_t".to_string(),
        "i64" => "int64_t".to_string(),
        "usize" | "size_t" => "size_t".to_string(),
        "isize" | "ssize_t" => "ssize_t".to_string(),
        "f32" => "float".to_string(),
        "f64" => "double".to_string(),
        "bool" => "bool".to_string(),
        "void" | "()" | "c_void" => "void".to_string(),
        "c_char" | "char" => "char".to_string(),
        "c_uchar" => "unsigned char".to_string(),
        "c_short" => "short".to_string(),
        "c_ushort" => "unsigned short".to_string(),
        "c_int" => "int".to_string(),
        "c_uint" => "unsigned int".to_string(),
        "c_long" => "long".to_string(),
        "c_ulong" => "unsigned long".to_string(),
        "c_longlong" => "long long".to_string(),
        "c_ulonglong" => "unsigned long long".to_string(),
        "c_float" => "float".to_string(),
        "c_double" => "double".to_string(),
        "intptr_t" => "intptr_t".to_string(),
        "uintptr_t" => "uintptr_t".to_string(),
        other => format!("Az{}", other),
    }
}

fn emit_one(out: &mut String, func: &FunctionDef, _ir: &CodegenIR) {
    let mut params: Vec<String> = Vec::new();
    let mut call_args: Vec<String> = Vec::new();

    for (idx, a) in func.args.iter().enumerate() {
        let raw_name = if a.name.is_empty() {
            format!("_arg{}", idx)
        } else {
            sanitize_c_arg(&a.name)
        };
        let c_ty = c_typename(&a.type_name);
        match a.ref_kind {
            ArgRefKind::Owned => {
                if let Some(cbi) = a.callback_info.as_ref().filter(|cbi| {
                    // The DLL ABI takes the BARE fn pointer (`Az<K>CallbackType`)
                    // exactly when the api.json arg passes the WRAPPER struct of a
                    // host-invoker kind (the typed-callback API change rewrote
                    // those setters). The Haskell side holds the wrapper struct
                    // (what `Az<K>Callback_createFromHostHandle` returns), so keep
                    // the struct-pointer parameter and forward its `cb` field.
                    // NOT rewritten:
                    //  - typedef-form args (`IconResolverCallbackType`): the plain
                    //    aggregate path already passes the fn ptr correctly;
                    //  - non-invoker wrapper structs (`DatasetMergeCallback`): the
                    //    DLL genuinely takes the struct by value.
                    a.type_name == cbi.callback_wrapper_name
                        && super::super::managed_host_invoker::HOST_INVOKER_KINDS
                            .contains(&cbi.callback_wrapper_name.as_str())
                }) {
                    let wrapper = c_typename(&cbi.callback_wrapper_name);
                    params.push(format!("const {} *{}", wrapper, raw_name));
                    call_args.push(format!("{}->cb", raw_name));
                } else if is_c_primitive(&a.type_name) {
                    params.push(format!("{} {}", c_ty, raw_name));
                    call_args.push(raw_name);
                } else {
                    // Aggregate by-value: shim takes `const T *`, derefs
                    // before calling.
                    params.push(format!("const {} *{}", c_ty, raw_name));
                    call_args.push(format!("*{}", raw_name));
                }
            }
            ArgRefKind::Ref => {
                params.push(format!("const {} *{}", c_ty, raw_name));
                call_args.push(raw_name);
            }
            ArgRefKind::RefMut | ArgRefKind::PtrMut => {
                params.push(format!("{} *{}", c_ty, raw_name));
                call_args.push(raw_name);
            }
            ArgRefKind::Ptr => {
                params.push(format!("const {} *{}", c_ty, raw_name));
                call_args.push(raw_name);
            }
        }
    }

    let returns_void = match &func.return_type {
        None => true,
        Some(r) => matches!(r.trim(), "" | "void" | "()" | "c_void"),
    };
    let ret_aggregate = return_is_aggregate(func);

    if ret_aggregate {
        let r = func.return_type.as_deref().unwrap();
        let c_r = c_typename(r);
        params.push(format!("{} *az_out", c_r));
        out.push_str(&format!(
            "void {}_via({}) {{ *az_out = {}({}); }}\n",
            func.c_name,
            params.join(", "),
            func.c_name,
            call_args.join(", ")
        ));
    } else if returns_void {
        out.push_str(&format!(
            "void {}_via({}) {{ {}({}); }}\n",
            func.c_name,
            params.join(", "),
            func.c_name,
            call_args.join(", ")
        ));
    } else {
        let r = func.return_type.as_deref().unwrap();
        let c_r = c_typename(r);
        out.push_str(&format!(
            "{} {}_via({}) {{ return {}({}); }}\n",
            c_r,
            func.c_name,
            params.join(", "),
            func.c_name,
            call_args.join(", ")
        ));
    }
}

fn sanitize_c_arg(name: &str) -> String {
    let mut s = String::with_capacity(name.len());
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            s.push(c);
        } else {
            s.push('_');
        }
    }
    // C reserved words / common-conflict names.
    match s.as_str() {
        "default" | "register" | "extern" | "static" | "auto" | "const" | "volatile"
        | "restrict" | "inline" | "typedef" | "struct" | "union" | "enum" | "if" | "else"
        | "while" | "for" | "do" | "return" | "switch" | "case" | "break" | "continue" => {
            format!("{}_", s)
        }
        _ => s,
    }
}
