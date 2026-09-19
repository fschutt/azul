//! Shared classification helpers for managed-FFI language adapters.
//!
//! "Managed-FFI" languages — Lua, Ruby, Perl, PHP, OCaml, Node, etc. — load
//! a prebuilt `libazul` at runtime and want their wrappers to feel as
//! idiomatic as Python's. The hard work of identifying which API functions
//! take callback typedef arguments is already done in the IR: every
//! [`FunctionArg`] whose `callback_info` is `Some` is a callback site.
//!
//! Adapters share this module instead of re-implementing the introspection.
//! Each language still emits its own host-side cast/pin idiom (LuaJIT
//! `ffi.cast`, Ruby `FFI::Function.new`, Perl `FFI::Platypus::Closure`,
//! OCaml `Foreign.funptr`, …) — those bindings are too different to factor
//! out into a single template.
//!
//! The helpers below are deliberately small. They answer:
//!
//! * "Does this function take any callback args?"
//! * "Which positional args of this function are callbacks?"
//! * "What is the C-ABI typedef name for this callback (so I can pass it to my language's cast/pin
//!   helper)?"
//!
//! Adapter files use these to expand their wrapper-method emitters from
//! the trivial `function(...) return C.fn(self, ...) end` form into
//! per-language idiomatic wrappers that auto-cast host closures.

use super::ir::{CodegenIR, FunctionArg, FunctionDef, FunctionKind, TypeCategory};

/// Return the subset of args that are callback typedef arguments.
///
/// "Callback typedef argument" means the IR builder annotated this arg
/// with [`CallbackArgInfo`] in `detect_callback_arg_info` (i.e. its type
/// ends with `CallbackType` and is not a destructor/clone callback).
///
/// The order is preserved.
pub fn callback_args(func: &FunctionDef) -> Vec<&FunctionArg> {
    func.args
        .iter()
        .filter(|a| a.callback_info.is_some())
        .collect()
}

/// True if any positional arg of `func` is a callback typedef.
///
/// Adapters use this as a fast-path: functions with no callback args can
/// keep using the simple varargs forwarder; only functions where this
/// returns `true` need the full per-arg expansion.
pub fn has_callback_arg(func: &FunctionDef) -> bool {
    func.args.iter().any(|a| a.callback_info.is_some())
}

/// Return the C-ABI typename for a callback arg (e.g. `"AzCallbackType"`).
///
/// This is the name the host-language cast helper (LuaJIT `ffi.cast`,
/// Ruby `FFI::Function.new` via the `callback :name, ...` lookup,
/// Perl `FFI::Platypus->closure`, …) needs to associate the host closure
/// with the right C signature.
///
/// `prefix` is the per-language type prefix, usually `"Az"`.
pub fn callback_typedef_c_name(arg: &FunctionArg, prefix: &str) -> Option<String> {
    arg.callback_info
        .as_ref()
        .map(|info| format!("{}{}", prefix, info.callback_typedef_name))
}

/// True if `func`'s receiver is the implicit `self`.
///
/// Adapters use this to decide whether to emit `function T:method(...)`
/// (instance method) or `T.method = function(...)` (static method).
pub fn takes_self(func: &FunctionDef) -> bool {
    use super::ir::FunctionKind;
    matches!(
        func.kind,
        FunctionKind::Method | FunctionKind::MethodMut | FunctionKind::DeepCopy
    )
}

// ============================================================================
// Wrapper-class classification (shared by C#, Java, Kotlin, ... so that the
// "a wrapper class exists" and "a typed delegate/SAM exists" decisions can
// never drift apart again — they did once, and 27 wrappers called a
// nonexistent `_delete`).
// ============================================================================

/// Does this struct get a managed-language wrapper class — an object with
/// methods and, when the C API has an `Az<T>_delete`, ownership?
///
/// Rule: a non-generic struct whose category is neither `Recursive`,
/// `VecRef`, `DestructorOrClone` nor `GenericTemplate`, and that has a
/// `_delete` OR at least one (mut) method in the C API. Per-target type
/// inclusion (`config.should_include_type`) stays with the caller.
pub fn has_wrapper_class(type_name: &str, ir: &CodegenIR) -> bool {
    let Some(s) = ir.find_struct(type_name.trim()) else {
        return false;
    };
    if !s.generic_params.is_empty() {
        return false;
    }
    if matches!(
        s.category,
        TypeCategory::Recursive
            | TypeCategory::VecRef
            | TypeCategory::DestructorOrClone
            | TypeCategory::GenericTemplate
    ) {
        return false;
    }
    has_delete_function(type_name, ir)
        || ir.functions.iter().any(|f| {
            f.class_name == type_name.trim()
                && matches!(f.kind, FunctionKind::Method | FunctionKind::MethodMut)
        })
}

/// Does the C API own native resources for this type, i.e. is there an
/// `Az<T>_delete`? Wrappers of such types get a finalizer / `Dispose` /
/// `close()`; wrappers of methods-only types (`CallbackInfo`, borrowed from
/// the engine for the duration of a callback; POD value types) MUST NOT —
/// there is nothing to call.
pub fn has_delete_function(type_name: &str, ir: &CodegenIR) -> bool {
    ir.functions
        .iter()
        .any(|f| f.class_name == type_name.trim() && matches!(f.kind, FunctionKind::Delete))
}

/// Is `type_name` the engine's `RefAny` (the type that carries host handles
/// and therefore gets auto-wrapped/unwrapped by managed bindings)? Derived
/// from the IR category — bindings must not compare against the literal
/// `"RefAny"` / `"AzRefAny"` / `"az_ref_any"` spellings.
pub fn is_refany_type(type_name: &str, ir: &CodegenIR) -> bool {
    ir.find_struct(type_name.trim())
        .is_some_and(|s| matches!(s.category, TypeCategory::RefAny))
}
