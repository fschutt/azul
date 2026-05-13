//! C shim layer for Haskell bindings.
//!
//! GHC's foreign-import doesn't support passing or returning C structs
//! by value. Every C-ABI function with an aggregate arg or return
//! therefore needs a wrapper that:
//!
//! - Takes by-value aggregate args as `const T *` (Haskell allocates +
//!   pokes; the shim dereferences before calling).
//! - Takes by-value aggregate returns as a trailing `T *__out`
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

use super::super::ir::{ArgRefKind, CodegenIR, FunctionDef, TypeCategory};
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
    out
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
    match t.trim() {
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
                if is_c_primitive(&a.type_name) {
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
        params.push(format!("{} *__out", c_r));
        out.push_str(&format!(
            "void {}_via({}) {{ *__out = {}({}); }}\n",
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
