//! Go managed-FFI callback layer (host-invoker pattern).
//!
//! Emits two files into the Go package:
//!
//! * `callbacks.go` — cgo preamble declaring libazul's host-invoker C ABI
//!   (the `AzApp_set<Kind>Invoker` / `Az<Kind>_createFromHostHandle` /
//!   `AzRefAny_newHostHandle` exports are NOT in `azul.h`, so this file
//!   declares them), a handle registry (`sync.Map` + atomic counter), the
//!   per-kind `Register<Kind>(fn)` helpers, `RefAnyWrap`/`RefAnyGet`,
//!   string helpers, smart factories (`NewWindowCreateOptions`,
//!   `NewAppWithData`, `RunWindow`, per-widget `On<Event>` setters), and
//!   the cross-package `Raw()` accessors.
//! * `callbacks_export.go` — the `//export` trampolines, one per invoker
//!   *arity* (total pointer parameters after the `uint64` handle). Files
//!   containing `//export` must not define anything in their cgo preamble
//!   (cgo copies the preamble into two generated C files), hence the
//!   separate file with a minimal `#include <stdint.h>` preamble.
//!
//! # How the dispatch works
//!
//! libazul's per-kind static thunk extracts the host handle from the
//! callback ctx `RefAny` and calls the registered per-kind invoker with
//! POINTER arguments only (see `azul-core/src/host_invoker.rs`). All
//! per-kind invokers of the same arity therefore share one machine-level
//! ABI, so `callbacks.go` registers the same exported Go trampoline
//! (cast per kind) for every kind of that arity. The trampoline forwards
//! `(handle, args...)` to `azGoDispatch`, which looks up the
//! `azGoAdapter` closure stored by `Register<Kind>` and runs it; the
//! adapter casts each pointer back to its C type, wraps borrowed views
//! in the wrapper structs from `wrappers.go`, calls the user's Go
//! function, and writes the result through the trailing out-pointer.
//!
//! # Cross-package rules this layer relies on
//!
//! cgo types are package-local: a consumer package can never NAME
//! `C.AzDom`, but it CAN hold and pass values of that type obtained from
//! this package via type inference. The Go-facing callback signatures
//! therefore use only wrapper types (`*RefAny`, `*CallbackInfo`, `*Dom`),
//! Go enums from `types.go` (`AzUpdate`), primitives, and
//! `unsafe.Pointer` — all namable by consumers. `Raw()` bridges wrapper
//! values back into the `C.Az*`-typed parameters of `wrappers.go`.
//!
//! # Known runtime caveat (documented, not fixable here)
//!
//! libazul's empty `Az*Vec`s carry Rust `NonNull::dangling()` sentinels
//! (small non-null values like `0x8`) in pointer fields. Go's stack-copy
//! invalid-pointer check aborts when such a by-value C struct is live on
//! a growing goroutine stack. Consumers must run with
//! `GODEBUG=invalidptr=0` (the documented cgo mitigation) — see
//! `examples/go/hello-world-idiomatic/main.go` for a self-contained
//! re-exec guard. The real fix is libazul-side: page-aligned (>= 0x1000)
//! dangling sentinels for FFI-visible empty vecs.

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{CodegenIR, FunctionKind};
use super::super::managed_host_invoker::{
    host_invoker_kinds, is_callback_wrapper, managed_c_symbol, wrapper_name,
};
use super::wrappers::should_emit_wrapper;

/// Map an IR callback-arg / return type name to its C-ABI name
/// (`usize` → `size_t`, everything else gets the `Az` prefix). Narrow
/// local variant of `managed_host_invoker::c_typename` — callback
/// typedef args in the IR only ever contain `usize` and Az-struct types.
fn c_typename(t: &str) -> String {
    if t == "usize" {
        "size_t".to_string()
    } else {
        format!("Az{}", t)
    }
}

/// `CheckBoxState` → `checkBoxState`.
fn lower_camel(t: &str) -> String {
    let mut chars = t.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_lowercase().collect::<String>() + chars.as_str(),
    }
}

/// Deterministic Go parameter name for the i-th callback argument.
fn arg_go_name(i: usize, t: &str) -> String {
    if i == 0 && t == "RefAny" {
        "data".to_string()
    } else if t.ends_with("CallbackInfo") {
        "info".to_string()
    } else if t == "usize" {
        "index".to_string()
    } else {
        lower_camel(t)
    }
}

/// Go-facing parameter type for a callback argument. Wrapper types come
/// from `wrappers.go`; types without a wrapper degrade to
/// `unsafe.Pointer` (still namable cross-package).
fn arg_go_type(t: &str, wrapper_types: &[String]) -> String {
    if t == "usize" {
        "uint".to_string()
    } else if wrapper_types.iter().any(|w| w.as_str() == t) {
        format!("*{}", t)
    } else {
        "unsafe.Pointer".to_string()
    }
}

/// Classification of a callback kind's return for the Go-facing surface.
enum RetKind {
    /// Callback returns nothing (e.g. `ThreadCallback`).
    Void,
    /// IR enum with a Go mirror type in `types.go` (`Update` → `AzUpdate`).
    Enum(String),
    /// Struct with a wrapper in `wrappers.go` — user returns `*<T>`,
    /// the adapter moves `inner` out and disarms the finalizer.
    Wrapper(String),
    /// No wrapper exists (e.g. `OnTextInputReturn`): the Go function
    /// gets a trailing `out unsafe.Pointer` parameter instead.
    OutParam(String),
}

fn ret_kind(ret: Option<&str>, ir: &CodegenIR, wrapper_types: &[String]) -> RetKind {
    let Some(r) = ret else { return RetKind::Void };
    if r == "void" || r == "()" {
        return RetKind::Void;
    }
    if ir.find_enum(r).is_some() {
        return RetKind::Enum(r.to_string());
    }
    if wrapper_types.iter().any(|w| w.as_str() == r) {
        return RetKind::Wrapper(r.to_string());
    }
    RetKind::OutParam(r.to_string())
}

/// Ordered wrapper-type list — must match `wrappers.rs` emission order
/// exactly (same filter, same `ir.structs` iteration) so the `Raw()`
/// block lines up with the artifact. Entries are the IR struct names
/// (identical to the Go wrapper names for every current type; Go-keyword
/// mangling never fires on Az type names).
fn wrapper_type_list(ir: &CodegenIR, config: &CodegenConfig) -> Vec<String> {
    ir.structs
        .iter()
        .filter(|s| should_emit_wrapper(s, ir, config))
        .map(|s| super::sanitize_identifier(&s.name))
        .collect()
}

/// One host-invoker kind, IR-derived.
struct Kind<'a> {
    wrapper: &'a str,
    arg_types: Vec<String>,
    ret: Option<String>,
}

fn kind_list<'a>(ir: &'a CodegenIR) -> Vec<Kind<'a>> {
    host_invoker_kinds(ir)
        .map(|cb| Kind {
            wrapper: wrapper_name(cb),
            arg_types: cb.args.iter().map(|a| a.type_name.trim().to_string()).collect(),
            ret: cb
                .return_type
                .as_deref()
                .filter(|r| *r != "void" && *r != "()")
                .map(|r| r.trim().to_string()),
        })
        .collect()
}

fn arity(k: &Kind) -> usize {
    k.arg_types.len() + usize::from(k.ret.is_some())
}

/// Sorted, deduplicated set of invoker arities present in the IR.
fn arity_set(kinds: &[Kind]) -> Vec<usize> {
    let mut v: Vec<usize> = kinds.iter().map(arity).collect();
    v.sort_unstable();
    v.dedup();
    v
}

// ============================================================================
// callbacks_export.go
// ============================================================================

/// Generate the contents of `callbacks_export.go`.
pub fn generate_export(ir: &CodegenIR, config: &CodegenConfig) -> Result<String> {
    let kinds = kind_list(ir);
    let arities = arity_set(&kinds);
    let mut b = CodeBuilder::new(&config.indent);

    b.line("// ============================================================================");
    b.line("// callbacks_export.go - cgo //export trampolines for the host-invoker layer.");
    b.line("// Auto-generated by azul-doc codegen v2 (lang_go). DO NOT EDIT MANUALLY.");
    b.line("// ============================================================================");
    b.line("//");
    b.line("// One exported function per invoker *arity* (total pointer parameters after");
    b.line("// the uint64 handle). libazul calls these through per-kind function-pointer");
    b.line("// casts registered in callbacks.go; every parameter is a pointer, so the");
    b.line("// machine-level ABI is identical for every kind of the same arity.");
    b.line("//");
    b.line("// This file deliberately carries a minimal cgo preamble: files containing");
    b.line("// //export directives must not define anything in their preamble (cgo copies");
    b.line("// it into two generated C files).");
    b.blank();
    b.line("package azul");
    b.blank();
    b.line("/*");
    b.line("#include <stdint.h>");
    b.line("*/");
    b.line("import \"C\"");
    b.blank();
    b.line("import (");
    b.line("    \"unsafe\"");
    b.line(")");
    b.blank();
    for n in &arities {
        let params: Vec<String> = (0..*n).map(|i| format!("p{} unsafe.Pointer", i)).collect();
        let fwd: Vec<String> = (0..*n).map(|i| format!("p{}", i)).collect();
        b.line(&format!("//export azGoInvoker{}", n));
        b.line(&format!(
            "func azGoInvoker{}(handle C.uint64_t, {}) {{",
            n,
            params.join(", ")
        ));
        b.line(&format!("    azGoDispatch(uint64(handle), {})", fwd.join(", ")));
        b.line("}");
        b.blank();
    }
    b.line("//export azGoHostHandleRelease");
    b.line("func azGoHostHandleRelease(id C.uint64_t) {");
    b.line("    azGoHandles.Delete(uint64(id))");
    b.line("}");
    Ok(b.finish())
}

// ============================================================================
// callbacks.go
// ============================================================================

/// Generate the contents of `callbacks.go`.
pub fn generate(ir: &CodegenIR, config: &CodegenConfig) -> Result<String> {
    let kinds = kind_list(ir);
    let arities = arity_set(&kinds);
    let wrapper_types = wrapper_type_list(ir, config);
    let mut b = CodeBuilder::new(&config.indent);

    emit_header(&mut b);
    emit_cgo_preamble(&mut b, &kinds, &arities);
    emit_registry(&mut b);
    emit_string_helpers(&mut b);
    emit_refany_helpers(&mut b);
    emit_register_fns(&mut b, ir, &kinds, &wrapper_types);
    emit_smart_helpers(&mut b, ir, config);
    emit_raw_accessors(&mut b, &wrapper_types);

    Ok(b.finish())
}

fn emit_header(b: &mut CodeBuilder) {
    b.line("// ============================================================================");
    b.line("// callbacks.go - Go-native callback registration (host-invoker pattern).");
    b.line("// Auto-generated by azul-doc codegen v2 (lang_go). DO NOT EDIT MANUALLY.");
    b.line("// ============================================================================");
    b.line("//");
    b.line("// libazul cannot call an arbitrary Go closure directly: the C ABI wants a");
    b.line("// static function pointer, and Go function values are not that. Instead,");
    b.line("// libazul exposes the *host-invoker* pattern (see azul-core host_invoker.rs):");
    b.line("//");
    b.line("//   1. At init(), this package registers ONE exported Go trampoline per");
    b.line("//      invoker arity (callbacks_export.go) via AzApp_set<Kind>Invoker, plus");
    b.line("//      a shared releaser via AzApp_setHostHandleReleaser.");
    b.line("//   2. Register<Kind>(fn) stores the Go function in a process-global");
    b.line("//      registry under a fresh uint64 handle and returns the C callback");
    b.line("//      wrapper struct built by Az<Kind>_createFromHostHandle(handle) - its");
    b.line("//      cb is a static thunk inside libazul, its ctx a RefAny carrying the");
    b.line("//      handle.");
    b.line("//   3. When the callback fires, libazul's thunk extracts the handle and");
    b.line("//      calls the registered trampoline with POINTER arguments only; the");
    b.line("//      adapter stored in the registry casts them back and calls your Go");
    b.line("//      function. Return values travel through an out-pointer.");
    b.line("//   4. When the last clone of the ctx RefAny drops, libazul fires the");
    b.line("//      releaser and the registry entry is removed.");
    b.line("//");
    b.line("// The same handle registry backs RefAnyWrap/RefAnyGet, so arbitrary Go");
    b.line("// values ride through libazul as app data with the identical lifetime story.");
    b.line("//");
    b.line("// Callback arguments are BORROWED views into libazul's callback frame:");
    b.line("// valid only for the duration of the call. Do not retain them and do not");
    b.line("// call Close() on them; Clone() what you need to keep.");
    b.blank();
    b.line("package azul");
    b.blank();
}

fn emit_cgo_preamble(b: &mut CodeBuilder, kinds: &[Kind], arities: &[usize]) {
    b.line("/*");
    b.line("#include <stdint.h>");
    b.line("#include \"azul.h\"");
    b.blank();
    b.line("// ---- host-invoker C ABI (exported by libazul; not declared in azul.h) ----");
    b.line("extern void AzApp_setHostHandleReleaser(void (*releaser)(uint64_t));");
    b.line("extern AzRefAny AzRefAny_newHostHandle(uint64_t id);");
    b.line("extern uint64_t AzRefAny_getHostHandle(const AzRefAny* refany);");
    b.blank();
    for k in kinds {
        let mut parts = vec!["uint64_t".to_string()];
        for a in &k.arg_types {
            parts.push(format!("const {}*", c_typename(a)));
        }
        if let Some(r) = &k.ret {
            parts.push(format!("{}*", c_typename(r)));
        }
        b.line(&format!(
            "typedef void (*AzGo{w}Invoker)({args});",
            w = k.wrapper,
            args = parts.join(", ")
        ));
        b.line(&format!(
            "extern void AzApp_set{w}Invoker(AzGo{w}Invoker);",
            w = k.wrapper
        ));
        b.line(&format!(
            "extern Az{w} Az{w}_createFromHostHandle(uint64_t);",
            w = k.wrapper
        ));
        b.blank();
    }
    b.line("// ---- Go trampolines (defined in callbacks_export.go via //export) ----");
    for n in arities {
        let params: Vec<String> = (0..*n).map(|i| format!("void* p{}", i)).collect();
        b.line(&format!(
            "extern void azGoInvoker{}(uint64_t handle, {});",
            n,
            params.join(", ")
        ));
    }
    b.line("extern void azGoHostHandleRelease(uint64_t id);");
    b.blank();
    b.line("// Registers the shared releaser plus one trampoline per callback kind.");
    b.line("// The function-pointer casts are safe: every parameter is a pointer (or");
    b.line("// the uint64 handle), so azGoInvokerN has the same machine-level ABI as");
    b.line("// every per-kind invoker of the same arity.");
    b.line("static void azGoRegisterInvokers(void) {");
    b.line("    AzApp_setHostHandleReleaser(azGoHostHandleRelease);");
    for k in kinds {
        b.line(&format!(
            "    AzApp_set{w}Invoker((AzGo{w}Invoker)azGoInvoker{n});",
            w = k.wrapper,
            n = arity(k)
        ));
    }
    b.line("}");
    b.line("*/");
    b.line("import \"C\"");
    b.blank();
    b.line("import (");
    b.line("    \"runtime\"");
    b.line("    \"sync\"");
    b.line("    \"sync/atomic\"");
    b.line("    \"unsafe\"");
    b.line(")");
    b.blank();
}

fn emit_registry(b: &mut CodeBuilder) {
    b.line("// ============================================================================");
    b.line("// Handle registry");
    b.line("// ============================================================================");
    b.blank();
    b.line("// azGoAdapter is the uniform shape every registered callback is stored as:");
    b.line("// the per-kind Register function wraps the user's typed Go function in a");
    b.line("// closure that casts the raw pointer arguments back to their C types.");
    b.line("type azGoAdapter func(args []unsafe.Pointer)");
    b.blank();
    b.line("// azGoHandles maps uint64 handles to either an azGoAdapter (callbacks) or");
    b.line("// an arbitrary user value (RefAnyWrap). Entries are removed when libazul");
    b.line("// fires the host-handle releaser (azGoHostHandleRelease).");
    b.line("var azGoHandles sync.Map");
    b.blank();
    b.line("// azGoNextHandle is the last allocated handle id. Ids start at 1; libazul");
    b.line("// reserves 0 as \"no handle\".");
    b.line("var azGoNextHandle uint64");
    b.blank();
    b.line("func azGoNewHandle(v any) uint64 {");
    b.line("    id := atomic.AddUint64(&azGoNextHandle, 1)");
    b.line("    azGoHandles.Store(id, v)");
    b.line("    return id");
    b.line("}");
    b.blank();
    b.line("func azGoDispatch(handle uint64, args ...unsafe.Pointer) {");
    b.line("    v, ok := azGoHandles.Load(handle)");
    b.line("    if !ok {");
    b.line("        return");
    b.line("    }");
    b.line("    if fn, ok := v.(azGoAdapter); ok {");
    b.line("        fn(args)");
    b.line("    }");
    b.line("}");
    b.blank();
    b.line("func init() {");
    b.line("    C.azGoRegisterInvokers()");
    b.line("}");
    b.blank();
}

fn emit_string_helpers(b: &mut CodeBuilder) {
    b.line("// ============================================================================");
    b.line("// String helpers (native Go <-> AzString boundary)");
    b.line("// ============================================================================");
    b.blank();
    b.line("var azGoEmptyByte byte");
    b.blank();
    b.line("// Str copies a Go string into a freshly allocated AzString. The returned");
    b.line("// value is consumed by whichever libazul call it is passed to.");
    b.line("func Str(s string) C.AzString {");
    b.line("    b := []byte(s)");
    b.line("    ptr := (*C.uint8_t)(unsafe.Pointer(&azGoEmptyByte))");
    b.line("    if len(b) > 0 {");
    b.line("        ptr = (*C.uint8_t)(unsafe.Pointer(&b[0]))");
    b.line("    }");
    b.line("    return C.AzString_fromUtf8(ptr, C.size_t(len(b)))");
    b.line("}");
    b.blank();
    b.line("// GoStr copies an AzString's UTF-8 bytes into a Go string. The AzString");
    b.line("// is only read, never consumed.");
    b.line("func GoStr(s C.AzString) string {");
    b.line("    if s.vec.ptr == nil || s.vec.len == 0 {");
    b.line("        return \"\"");
    b.line("    }");
    b.line("    return string(unsafe.Slice((*byte)(unsafe.Pointer(s.vec.ptr)), int(s.vec.len)))");
    b.line("}");
    b.blank();
    b.line("// NewString wraps a Go string in a managed *String.");
    b.line("func NewString(s string) *String {");
    b.line("    self := &String{ inner: Str(s) }");
    b.line("    runtime.SetFinalizer(self, func(x *String) { x.Close() })");
    b.line("    return self");
    b.line("}");
    b.blank();
    b.line("// String returns the wrapped UTF-8 bytes as a Go string (copies).");
    b.line("func (self *String) String() string {");
    b.line("    if self == nil {");
    b.line("        return \"\"");
    b.line("    }");
    b.line("    return GoStr(self.inner)");
    b.line("}");
    b.blank();
}

fn emit_refany_helpers(b: &mut CodeBuilder) {
    b.line("// ============================================================================");
    b.line("// RefAny wrap/get (arbitrary Go values as libazul app data)");
    b.line("// ============================================================================");
    b.blank();
    b.line("// RefAnyWrap stores an arbitrary Go value in the handle registry and wraps");
    b.line("// the handle in a RefAny. The value stays reachable until libazul drops the");
    b.line("// last clone of the RefAny, at which point the releaser removes the registry");
    b.line("// entry and the Go GC may collect it. Store a POINTER (e.g. *MyModel) if");
    b.line("// callbacks should observe mutations across invocations.");
    b.line("func RefAnyWrap(value any) *RefAny {");
    b.line("    id := azGoNewHandle(value)");
    b.line("    self := &RefAny{ inner: C.AzRefAny_newHostHandle(C.uint64_t(id)) }");
    b.line("    runtime.SetFinalizer(self, func(x *RefAny) { x.Close() })");
    b.line("    return self");
    b.line("}");
    b.blank();
    b.line("// RefAnyGet recovers the Go value previously wrapped via RefAnyWrap.");
    b.line("// Returns (nil, false) if the RefAny is not a host handle (e.g. it was");
    b.line("// created natively) or the handle has already been released.");
    b.line("func RefAnyGet(ref *RefAny) (any, bool) {");
    b.line("    if ref == nil {");
    b.line("        return nil, false");
    b.line("    }");
    b.line("    id := uint64(C.AzRefAny_getHostHandle(&ref.inner))");
    b.line("    if id == 0 {");
    b.line("        return nil, false");
    b.line("    }");
    b.line("    return azGoHandles.Load(id)");
    b.line("}");
    b.blank();
}

fn emit_register_fns(
    b: &mut CodeBuilder,
    ir: &CodegenIR,
    kinds: &[Kind],
    wrapper_types: &[String],
) {
    b.line("// ============================================================================");
    b.line("// Per-kind callback registration");
    b.line("// ============================================================================");
    b.blank();

    for k in kinds {
        let rk = ret_kind(k.ret.as_deref(), ir, wrapper_types);
        let names: Vec<String> = k
            .arg_types
            .iter()
            .enumerate()
            .map(|(i, t)| arg_go_name(i, t))
            .collect();
        let mut params: Vec<String> = names
            .iter()
            .zip(&k.arg_types)
            .map(|(n, t)| format!("{} {}", n, arg_go_type(t, wrapper_types)))
            .collect();
        if matches!(rk, RetKind::OutParam(_)) {
            params.push("out unsafe.Pointer".to_string());
        }
        let sig = params.join(", ");

        b.line(&format!(
            "// {}Func is the Go signature for {} callbacks. Pointer",
            k.wrapper, k.wrapper
        ));
        b.line("// arguments are borrowed views into libazul's callback frame: valid only");
        b.line("// for the duration of the call - do not retain or Close them.");
        if let RetKind::OutParam(r) = &rk {
            b.line(&format!(
                "// The result must be written through `out` (*C.{}); no Go",
                c_typename(r)
            ));
            b.line("// wrapper type exists for this return type yet.");
        }
        match &rk {
            RetKind::Enum(r) => b.line(&format!("type {}Func func({}) Az{}", k.wrapper, sig, r)),
            RetKind::Wrapper(r) => b.line(&format!("type {}Func func({}) *{}", k.wrapper, sig, r)),
            RetKind::Void | RetKind::OutParam(_) => {
                b.line(&format!("type {}Func func({})", k.wrapper, sig))
            }
        }
        b.blank();
        b.line(&format!(
            "// Register{} wraps a Go function in a C Az{}",
            k.wrapper, k.wrapper
        ));
        b.line("// callback struct whose ctx carries a handle into the Go registry. Pass");
        b.line("// the result to any C-ABI parameter of that callback type.");
        b.line(&format!(
            "func Register{w}(fn {w}Func) C.Az{w} {{",
            w = k.wrapper
        ));
        b.line("    id := azGoNewHandle(azGoAdapter(func(args []unsafe.Pointer) {");
        for (i, (nm, t)) in names.iter().zip(&k.arg_types).enumerate() {
            if t == "usize" {
                b.line(&format!("        {} := uint(*(*C.size_t)(args[{}]))", nm, i));
            } else if wrapper_types.iter().any(|w| w == t) {
                b.line(&format!(
                    "        {} := &{}{{ inner: *(*C.{})(args[{}]) }}",
                    nm,
                    t,
                    c_typename(t),
                    i
                ));
            } else {
                b.line(&format!("        {} := args[{}]", nm, i));
            }
        }
        let call_args = names.join(", ");
        let n_args = k.arg_types.len();
        match &rk {
            RetKind::Void => b.line(&format!("        fn({})", call_args)),
            RetKind::OutParam(_) => {
                b.line(&format!("        fn({}, args[{}])", call_args, n_args))
            }
            RetKind::Enum(r) => b.line(&format!(
                "        *(*C.{c})(args[{n}]) = C.{c}(fn({a}))",
                c = c_typename(r),
                n = n_args,
                a = call_args
            )),
            RetKind::Wrapper(r) => {
                b.line(&format!("        ret := fn({})", call_args));
                b.line("        if ret != nil {");
                b.line(&format!(
                    "            *(*C.{})(args[{}]) = ret.inner",
                    c_typename(r),
                    n_args
                ));
                b.line("            runtime.SetFinalizer(ret, nil)");
                b.line("        }");
            }
        }
        b.line("    }))");
        b.line(&format!(
            "    return C.Az{w}_createFromHostHandle(C.uint64_t(id))",
            w = k.wrapper
        ));
        b.line("}");
        b.blank();
    }
}

fn emit_smart_helpers(b: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    b.line("// ============================================================================");
    b.line("// Smart factories and setters (Go-native surface)");
    b.line("// ============================================================================");
    b.blank();
    // Smart layout factory. Field path `window_state.layout_callback`
    // matches `managed_host_invoker::layout_callback_factory_info`
    // (WindowCreateOptions is the only class matching the pattern today);
    // kept literal here because cgo needs the concrete field-access
    // expression anyway.
    b.line("// NewWindowCreateOptions builds WindowCreateOptions whose layout callback");
    b.line("// is the given Go function (host-invoker registered, ctx-preserving).");
    b.line("func NewWindowCreateOptions(fn LayoutCallbackFunc) *WindowCreateOptions {");
    b.line("    wco := C.AzWindowCreateOptions_default()");
    b.line("    wco.window_state.layout_callback = RegisterLayoutCallback(fn)");
    b.line("    self := &WindowCreateOptions{ inner: wco }");
    b.line("    runtime.SetFinalizer(self, func(x *WindowCreateOptions) { x.Close() })");
    b.line("    return self");
    b.line("}");
    b.blank();
    b.line("// NewAppWithData creates an App whose app data is an arbitrary Go value");
    b.line("// (wrapped via RefAnyWrap). Pass nil config for the default AppConfig.");
    b.line("func NewAppWithData(data any, config *AppConfig) *App {");
    b.line("    var cfg C.AzAppConfig");
    b.line("    if config != nil {");
    b.line("        cfg = config.inner");
    b.line("        runtime.SetFinalizer(config, nil)");
    b.line("    } else {");
    b.line("        cfg = C.AzAppConfig_create()");
    b.line("    }");
    b.line("    ref := RefAnyWrap(data)");
    b.line("    inner := ref.inner");
    b.line("    runtime.SetFinalizer(ref, nil)");
    b.line("    self := &App{ inner: C.AzApp_create(inner, cfg) }");
    b.line("    runtime.SetFinalizer(self, func(x *App) { x.Close() })");
    b.line("    return self");
    b.line("}");
    b.blank();
    b.line("// RunWindow consumes win and enters the main loop.");
    b.line("func (self *App) RunWindow(win *WindowCreateOptions) {");
    b.line("    inner := win.inner");
    b.line("    runtime.SetFinalizer(win, nil)");
    b.line("    C.AzApp_run(&self.inner, inner)");
    b.line("}");
    b.blank();

    // Per-widget On<Event> smart setters: for every instance method
    // `set_on_x(self, data: RefAny, cb: <Kind>)` whose kind is in the
    // host-invoker allowlist, emit `On<X>(data *RefAny, fn <Kind>Func)`.
    // Iterated per wrapper struct (same order as wrappers.go) so the
    // artifact lines up.
    for s in ir.structs.iter().filter(|s| should_emit_wrapper(s, ir, config)) {
        let go_name = super::sanitize_identifier(&s.name);
        for f in ir.functions_for_class(&s.name) {
            if !matches!(f.kind, FunctionKind::Method | FunctionKind::MethodMut) {
                continue;
            }
            if f.args.len() != 3 {
                continue;
            }
            if f.args[1].type_name.trim() != "RefAny" {
                continue;
            }
            let cb_ty = f.args[2].type_name.trim();
            if !is_callback_wrapper(cb_ty) {
                continue;
            }
            let Some(rest) = f.method_name.strip_prefix("set_on_") else {
                continue;
            };
            let method = format!("On{}", super::snake_to_pascal(rest));
            b.line(&format!(
                "// {} registers a Go function as the {} handler. `data` is",
                method, method
            ));
            b.line("// cloned; the caller's RefAny stays valid.");
            b.line(&format!(
                "func (self *{}) {}(data *RefAny, fn {}Func) {{",
                go_name, method, cb_ty
            ));
            b.line(&format!(
                "    C.{}(&self.inner, C.AzRefAny_clone(&data.inner), Register{}(fn))",
                managed_c_symbol(f),
                cb_ty
            ));
            b.line("}");
            b.blank();
        }
    }
}

fn emit_raw_accessors(b: &mut CodeBuilder, wrapper_types: &[String]) {
    b.line("// ============================================================================");
    b.line("// Raw accessors (cross-package bridge)");
    b.line("// ============================================================================");
    b.line("//");
    b.line("// cgo types are package-local: a consumer package cannot NAME C.AzDom, but");
    b.line("// it CAN hold and pass values of that type obtained from this package via");
    b.line("// type inference. Raw() hands out the underlying C value and disarms the");
    b.line("// wrapper's finalizer (ownership transfer): pass the result to a consuming");
    b.line("// libazul parameter (AddChild, RunWindow, ...). Clone() the wrapper first");
    b.line("// if you still need it afterwards.");
    b.blank();
    for t in wrapper_types {
        b.line(&format!(
            "// Raw returns the underlying C.Az{} value, transferring ownership to",
            t
        ));
        b.line("// the caller (the wrapper's finalizer, if any, is disarmed).");
        b.line(&format!("func (self *{t}) Raw() C.Az{t} {{", t = t));
        b.line("    runtime.SetFinalizer(self, nil)");
        b.line("    return self.inner");
        b.line("}");
        b.blank();
    }
}
