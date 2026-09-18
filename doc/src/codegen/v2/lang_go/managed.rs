//! Go managed-FFI callback layer (host-invoker pattern).
//!
//! Emits two files into the Go package:
//!
//! * `callbacks.go` — cgo preamble declaring libazul's host-invoker C ABI (the
//!   `AzApp_set<Kind>Invoker` / `Az<Kind>_createFromHostHandle` / `AzRefAny_newHostHandle` exports
//!   are NOT in `azul.h`, so this file declares them), a handle registry (`sync.Map` + atomic
//!   counter), the per-kind `Register<Kind>(fn)` helpers, `RefAnyWrap`/`RefAnyGet`, string helpers,
//!   smart factories (`NewWindowCreateOptions`, `NewAppWithData`, `RunWindow`, per-widget
//!   `On<Event>` setters), and the cross-package `Raw()` accessors.
//! * `callbacks_export.go` — the `//export` trampolines, one per invoker *arity* (total pointer
//!   parameters after the `uint64` handle). Files containing `//export` must not define anything in
//!   their cgo preamble (cgo copies the preamble into two generated C files), hence the separate
//!   file with a minimal `#include <stdint.h>` preamble.
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
//! # No C types on the Go side
//!
//! The preamble includes azul.h (the host-invoker exports are declared
//! against `AzRefAny` etc.), but Go never names a C type: the two calls
//! that return a struct by value (`AzRefAny_newHostHandle`,
//! `Az<Kind>_createFromHostHandle`) go through `static inline` shims that
//! write into Go-owned memory, and every pointer crosses as
//! `unsafe.Pointer`. Callback arguments are cast to the Go-native `Az*`
//! types from `types.go`; the Go-facing signatures use wrapper types
//! (`*RefAny`, `*CallbackInfo`, `*Dom`), native enums (`AzUpdate`),
//! primitives, and `unsafe.Pointer`. `Raw()` hands out the native value
//! (`AzDom`) for the raw-layer and wrapper parameters.
//!


use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{CodegenIR, FunctionKind};
use super::super::managed_host_invoker::{host_invoker_kinds, is_callback_wrapper, wrapper_name};
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

/// Go-native type name of a callback arg / return (`Dom` -> `AzDom`).
fn go_native(t: &str) -> String {
    format!("Az{}", t)
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
    } else if t == "RefAny" {
        "any".to_string()
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
            arg_types: cb
                .args
                .iter()
                .map(|a| a.type_name.trim().to_string())
                .collect(),
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
    b.line("// callbacks_export.go - dynamic trampolines for the host-invoker layer.");
    b.line("// Auto-generated by azul-doc codegen v2 (lang_go). DO NOT EDIT MANUALLY.");
    b.line("// ============================================================================");
    b.line("//");
    b.line("// One function per invoker *arity* (total pointer parameters after");
    b.line("// the uint64 handle). libazul calls these through per-kind function-pointer");
    b.line("// casts registered in callbacks.go.");
    b.blank();
    b.line("package azul");
    b.blank();
    b.line("import \"unsafe\"");
    b.blank();
    b.blank();
    for n in &arities {
        let params: Vec<String> = (0..*n).map(|i| format!("p{} uintptr", i)).collect();
        let fwd: Vec<String> = (0..*n).map(|i| format!("unsafe.Pointer(p{})", i)).collect();
        b.line(&format!(
            "func azGoInvoker{}(handle uint64, {}) uintptr {{",
            n,
            params.join(", ")
        ));
        b.line(&format!(
            "    azGoDispatch(handle, {})",
            fwd.join(", ")
        ));
        b.line("    return 0");
        b.line("}");
        b.blank();
    }
    b.line("func azGoHostHandleRelease(id uint64) uintptr {");
    b.line("    azGoHandles.Delete(id)");
    b.line("    return 0");
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
    emit_raw_accessors(&mut b, ir, &wrapper_types);

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
    b.line("import (\n    \"log\"\n    \"runtime\"\n    \"sync\"\n    \"sync/atomic\"\n    \"unsafe\"\n    \"github.com/ebitengine/purego\"\n)");
    b.blank();
}

fn emit_cgo_preamble(b: &mut CodeBuilder, kinds: &[Kind], arities: &[usize]) {
    b.line("var libAzApp_setHostHandleReleaser func(uintptr)");
    b.line("var libAzRefAny_newHostHandleByref func(uint64, uintptr)");
    b.line("var libAzRefAny_getHostHandle func(uintptr) uint64");
    b.blank();
    for k in kinds {
        b.line(&format!("var libAzApp_set{w}Invoker func(uintptr)", w = k.wrapper));
        b.line(&format!("var libAz{w}_createFromHostHandleByref func(uint64, uintptr)", w = k.wrapper));
    }
    b.blank();
    
    b.line("func azGoRefAnyNewHostHandle(id uint64, out unsafe.Pointer) {");
    b.line("    libAzRefAny_newHostHandleByref(id, uintptr(out))");
    b.line("}");
    b.blank();
    
    for k in kinds {
        b.line(&format!("func azGoCreate{w}(id uint64, out unsafe.Pointer) {{", w = k.wrapper));
        b.line(&format!("    libAz{w}_createFromHostHandleByref(id, uintptr(out))", w = k.wrapper));
        b.line("}");
    }
    b.blank();
    
    b.line("func initCallbacks(lib uintptr) {");
    b.indent();
    b.line("purego.RegisterLibFunc(&libAzApp_setHostHandleReleaser, lib, \"AzApp_setHostHandleReleaser\")");
    b.line("purego.RegisterLibFunc(&libAzRefAny_newHostHandleByref, lib, \"AzRefAny_newHostHandleByref\")");
    b.line("purego.RegisterLibFunc(&libAzRefAny_getHostHandle, lib, \"AzRefAny_getHostHandle\")");
    for k in kinds {
        b.line(&format!("purego.RegisterLibFunc(&libAzApp_set{w}Invoker, lib, \"AzApp_set{w}Invoker\")", w = k.wrapper));
        b.line(&format!("purego.RegisterLibFunc(&libAz{w}_createFromHostHandleByref, lib, \"Az{w}_createFromHostHandleByref\")", w = k.wrapper));
    }
    b.blank();
    b.line("releaserCB := purego.NewCallback(azGoHostHandleRelease)");
    b.line("libAzApp_setHostHandleReleaser(releaserCB)");
    b.blank();
    for n in arities {
        b.line(&format!("invoker{}CB := purego.NewCallback(azGoInvoker{})", n, n));
    }
    b.blank();
    for k in kinds {
        b.line(&format!("libAzApp_set{w}Invoker(invoker{n}CB)", w = k.wrapper, n = arity(k)));
    }
    b.dedent();
    b.line("}");
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
    b.line("func Str(s string) AzString {");
    b.line("    b := []byte(s)");
    b.line("    ptr := &azGoEmptyByte");
    b.line("    if len(b) > 0 {");
    b.line("        ptr = &b[0]");
    b.line("    }");
    b.line("    return AzString_fromUtf8(ptr, uintptr(len(b)))");
    b.line("}");
    b.blank();
    b.line("// GoStr copies an AzString's UTF-8 bytes into a Go string. The AzString");
    b.line("// is only read, never consumed.");
    b.line("func GoStr(s AzString) string {");
    b.line("    if s.Vec.Ptr == nil || s.Vec.Len == 0 {");
    b.line("        return \"\"");
    b.line("    }");
    b.line("    return string(unsafe.Slice((*byte)(s.Vec.Ptr), int(s.Vec.Len)))");
    b.line("}");
    b.blank();
    b.line("// NewString wraps a Go string in a managed *String.");
    b.line("func NewString(s string) *String {");
    b.line("    inner := Str(s)");
    b.line("    self := &String{ inner: &inner }");
    b.line("    runtime.SetFinalizer(self, func(x *String) { x.Close() })");
    b.line("    return self");
    b.line("}");
    b.blank();
    // A managed `*String` had no way to read its own text. The obvious name,
    // `String()`, is taken: api.json gives the class `Debug`, so the wrapper
    // generator emits a `String()` returning `AzString_toDbgString`, and two
    // methods of that name in one package do not compile. Deleting the
    // content accessor resolved the clash and left `fmt.Println(NewString(..))`
    // printing a Rust `{:#?}` dump with no way to get at the text at all.
    b.line("// Value returns the string's contents.");
    b.line("//");
    b.line("// Not named String(): that method exists on this type as the fmt.Stringer");
    b.line("// debug form (AzString_toDbgString), because api.json declares Debug for");
    b.line("// the class. Use Value() for the text and String() to inspect it.");
    b.line("func (self *String) Value() string {");
    b.line("    if self == nil || self.inner == nil {");
    b.line("        return \"\"");
    b.line("    }");
    b.line("    return GoStr(*self.inner)");
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
    b.line("    if r, ok := value.(*RefAny); ok {");
    b.line("        return r");
    b.line("    }");
    b.line("    id := azGoNewHandle(value)");
    b.line("    var inner AzRefAny");
    b.line("    azGoRefAnyNewHostHandle(id, unsafe.Pointer(&inner))");
    b.line("    self := &RefAny{ inner: &inner }");
    b.line("    runtime.SetFinalizer(self, func(x *RefAny) { x.Close() })");
    b.line("    return self");
    b.line("}");
    b.blank();
    b.line("// RefAnyGet recovers the Go value previously wrapped via RefAnyWrap.");
    b.line("// Returns (nil, false) if the RefAny is not a host handle (e.g. it was");
    b.line("// created natively) or the handle has already been released.");
    b.line("func RefAnyGet(ref *RefAny) (any, bool) {");
    b.line("    if ref == nil || ref.inner == nil {");
    b.line("        return nil, false");
    b.line("    }");
    b.line("    id := libAzRefAny_getHostHandle(uintptr(unsafe.Pointer(ref.inner)))");
    b.line("    if id == 0 {");
    b.line("        return nil, false");
    b.line("    }");
    b.line("    return azGoHandles.Load(id)");
    b.line("}");
    b.blank();
    b.line("// Bind automatically downcasts the RefAny to your specific model type T.");
    b.line("// If the downcast fails, it logs an error and returns the zero value for the return type (e.g., AzUpdate_DoNothing, or nil for *Dom).");
    b.line("func Bind[T any, Ctx any, Ret any](cb func(*T, Ctx) Ret) func(any, Ctx) Ret {");
    b.line("    return func(data any, ctx Ctx) Ret {");
    b.line("        var zero Ret");
    b.line("        model, ok := data.(*T)");
    b.line("        if !ok {");
    b.line("            log.Printf(\"azul.Bind: type assertion failed, expected %T, got %T\n\", new(T), data)");
    b.line("            return zero");
    b.line("        }");
    b.line("        return cb(model, ctx)");
    b.line("    }");
    b.line("}");
    b.blank();}

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
                "// The result must be written through `out` (*{}); no Go",
                go_native(r)
            ));
            b.line("// wrapper type exists for this return type yet.");
        }
        match &rk {
            RetKind::Enum(_) | RetKind::Wrapper(_) => b.line(&format!("type {}Func func({}) any", k.wrapper, sig)),
            RetKind::Void | RetKind::OutParam(_) => {
                b.line(&format!("type {}Func func({})", k.wrapper, sig))
            }
        }
        b.blank();
        b.line(&format!(
            "// Register{} wraps a Go function in an Az{}",
            k.wrapper, k.wrapper
        ));
        b.line("// callback struct whose ctx carries a handle into the Go registry. Pass");
        b.line("// the result to any parameter of that callback type.");
        b.line(&format!(
            "func Register{w}(fn {w}Func) Az{w} {{",
            w = k.wrapper
        ));
        b.line("    id := azGoNewHandle(azGoAdapter(func(args []unsafe.Pointer) {");
        for (i, (nm, t)) in names.iter().zip(&k.arg_types).enumerate() {
            if t == "usize" {
                b.line(&format!(
                    "        {} := uint(*(*uintptr)(args[{}]))",
                    nm, i
                ));
            } else if t == "RefAny" {
                b.line(&format!("        {}_wrap := &RefAny{{ inner: (*AzRefAny)(args[{}]), borrowed: true }}", nm, i));
                b.line(&format!("        {}, _ := RefAnyGet({}_wrap)", nm, nm));
            } else if wrapper_types.iter().any(|w| w == t) {
                let has_del = ir
                    .functions
                    .iter()
                    .any(|f| f.class_name == *t && f.kind == FunctionKind::Delete);
                let inner_expr = if has_del {
                    format!("(*{})(args[{}])", go_native(t), i)
                } else {
                    format!("*(*{})(args[{}])", go_native(t), i)
                };
                // `args[i]` points into the host invoker's argument array,
                // which was never malloc'd: this wrapper borrows it.
                let borrowed = if has_del { ", borrowed: true" } else { "" };
                b.line(&format!(
                    "        {} := &{}{{ inner: {}{} }}",
                    nm, t, inner_expr, borrowed
                ));
            } else {
                b.line(&format!("        {} := args[{}]", nm, i));
            }
        }
        let call_args = names.join(", ");
        let n_args = arity(k);
        match &rk {
            RetKind::Void => b.line(&format!("        fn({})", call_args)),
            RetKind::OutParam(_) => b.line(&format!("        fn({}, args[{}])", call_args, n_args)),
            RetKind::Enum(r) => {
                b.line(&format!("        ret := fn({})", call_args));
                b.line(&format!("        var out {c}", c = go_native(r)));
                b.line("        if ret == nil {");
                b.line("            out = 0");
                b.line(&format!("        }} else if v, ok := ret.({c}); ok {{", c = go_native(r)));
                b.line("            out = v");
                b.line("        } else {");
                b.line(&format!("            log.Printf(\"azul: callback returned junk value %T, expected {c} or nil\\n\", ret)", c = go_native(r)));
                b.line("            out = 0");
                b.line("        }");
                b.line(&format!("        *(*{c})(args[{n}]) = out", c = go_native(r), n = n_args));
            }
            RetKind::Wrapper(r) => {
                let has_del = ir
                    .functions
                    .iter()
                    .any(|f| f.class_name == *r && f.kind == FunctionKind::Delete);
                b.line(&format!("        ret := fn({})", call_args));
                b.line("        if ret == nil {");
                b.line("            // do nothing, out parameter is already zero-initialized");
                b.line(&format!("        }} else if v, ok := ret.(*{r}); ok {{"));
                if has_del {
                    b.line(&format!("            *(*{})(args[{}]) = *v.inner", go_native(r), n_args));
                    b.line("            v.inner = nil");
                } else {
                    b.line(&format!("            *(*{})(args[{}]) = v.inner", go_native(r), n_args));
                }
                b.line("            runtime.SetFinalizer(v, nil)");
                b.line("        } else {");
                b.line(&format!("            log.Printf(\"azul: callback returned junk value %T, expected *{r} or nil\\n\", ret)"));
                b.line("        }");
            }
        }
        b.line("    }))");
        b.line(&format!("    var out Az{w}", w = k.wrapper));
        b.line(&format!(
            "    azGoCreate{w}(id, unsafe.Pointer(&out))",
            w = k.wrapper
        ));
        b.line("    return out");
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
    b.line("    wco := AzWindowCreateOptions_createDefault()");
    b.line("    wco.WindowState.LayoutCallback = RegisterLayoutCallback(fn)");
    b.line("    self := &WindowCreateOptions{ inner: &wco }");
    b.line("    runtime.SetFinalizer(self, func(x *WindowCreateOptions) { x.Close() })");
    b.line("    return self");
    b.line("}");
    b.blank();

    b.line("// RunWindow consumes win and enters the main loop.");
    b.line("func (self *App) RunWindow(win *WindowCreateOptions) {");
    b.line("    inner := *win.inner");
    b.line("    win.inner = nil");
    b.line("    runtime.SetFinalizer(win, nil)");
    b.line("    AzApp_run(self.inner, inner)");
    b.line("}");
    b.blank();

    // Per-widget On<Event> smart setters: for every instance method
    // `set_on_x(self, data: RefAny, cb: <Kind>)` whose kind is in the
    // host-invoker allowlist, emit `On<X>(data *RefAny, fn <Kind>Func)`.
    // Iterated per wrapper struct (same order as wrappers.go) so the
    // artifact lines up.
    for s in ir
        .structs
        .iter()
        .filter(|s| should_emit_wrapper(s, ir, config))
    {
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
                "func (self *{}) {}(data any, fn {}Func) {{",
                go_name, method, cb_ty
            ));
            let self_has_del = ir
                .functions
                .iter()
                .any(|f| f.class_name == s.name && f.kind == FunctionKind::Delete);
            let self_expr = if self_has_del {
                "self.inner"
            } else {
                "&self.inner"
            };
            b.line(&format!(
                "    {}({}, AzRefAny_clone(RefAnyWrap(data).inner), Register{}(fn))",
                f.c_name,
                self_expr,
                cb_ty
            ));
            b.line("}");
            b.blank();
        }
    }
}

fn emit_raw_accessors(b: &mut CodeBuilder, ir: &CodegenIR, wrapper_types: &[String]) {
    b.line("// ============================================================================");
    b.line("// Raw accessors (cross-package bridge)");
    b.line("// ============================================================================");
    b.line("//");
    b.line("// Raw() hands out the underlying Go-native Az* value and disarms the");
    b.line("// wrapper's finalizer (ownership transfer): pass the result to a consuming");
    b.line("// libazul parameter (AddChild, RunWindow, ...). Clone() the wrapper first");
    b.line("// if you still need it afterwards.");
    b.blank();
    for t in wrapper_types {
        b.line(&format!(
            "// Raw returns the underlying Az{} value, transferring ownership to",
            t
        ));
        b.line("// the caller (the wrapper's finalizer, if any, is disarmed).");
        b.line(&format!("func (self *{t}) Raw() Az{t} {{", t = t));
        b.line("    runtime.SetFinalizer(self, nil)");

        let has_delete = ir.functions.iter().any(|f| f.class_name == *t && f.kind == FunctionKind::Delete);
        if has_delete {
            b.line("    val := *self.inner");
            b.line("    self.inner = nil");
            b.line("    return val");
        } else {
            b.line("    return self.inner");
        }
        b.line("}");
        b.blank();
    }
}
