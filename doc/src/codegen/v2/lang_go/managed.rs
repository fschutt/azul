//! Go managed-FFI callback layer (host-invoker pattern), purego edition.
//!
//! Emits two files into the Go package:
//!
//! * `callbacks.go` — the purego bindings of libazul's host-invoker exports
//!   (`AzApp_set<Kind>Invoker`, `Az<Kind>_createFromHostHandleByref`,
//!   `AzRefAny_newHostHandleByref`, `AzRefAny_getHostHandle`,
//!   `AzApp_setHostHandleReleaser`), a handle registry (`sync.Map` + atomic
//!   counter), the per-kind `Register<Kind>(fn)` helpers, `Bind`, `Str`,
//!   `RefAnyWrap`/`RefAnyGet`, the smart layout factory and per-widget
//!   `On<Event>` setters, and the cross-package `Raw()` accessors.
//! * `callbacks_trampolines.go` — the Go functions libazul calls back into,
//!   one per invoker *arity* (pointer parameters after the `uint64`
//!   handle) plus the host-handle releaser. `LoadLibrary` turns each into a
//!   C function pointer with `purego.NewCallback` exactly once.
//!
//! # How the dispatch works
//!
//! libazul's per-kind static thunk extracts the host handle from the
//! callback ctx `RefAny` and calls the registered per-kind invoker with
//! POINTER arguments only (see `azul-core/src/host_invoker.rs`). All
//! per-kind invokers of the same arity therefore share one machine-level
//! ABI, so `azInitCallbacks` registers the same trampoline (cast per kind)
//! for every kind of that arity. The trampoline forwards `(handle,
//! args...)` to `azGoDispatch`, which looks up the `azGoAdapter` closure
//! stored by `Register<Kind>` and runs it; the adapter casts each pointer
//! back to its Go-native type, wraps borrowed views in the wrapper structs
//! from `wrappers.go`, calls the user's Go function, and writes the result
//! through the trailing out-pointer — or leaves it alone when the Go
//! function returned nil, in which case libazul's pre-filled default for
//! that kind stands.
//!
//! # No C types, no cgo
//!
//! Everything crosses as Go-native `Az*` values from `types.go`: the two
//! exports that produce a struct (`AzRefAny_newHostHandleByref`,
//! `Az<Kind>_createFromHostHandleByref`) write into Go-owned memory
//! through an out-pointer. The Go-facing signatures use wrapper types
//! (`*CallbackInfo`, `*Dom`), `any` for the RefAny payload, native enums
//! (`AzUpdate`), primitives, and `unsafe.Pointer`. `Raw()` hands out the
//! native value (`AzDom`) for the raw-layer and wrapper parameters.

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{CodegenIR, FunctionKind, TypeCategory};
use super::super::managed_host_invoker::{
    host_invoker_kinds, is_callback_wrapper, layout_callback_factory_info, wrapper_name,
};
use super::super::managed_lang_helpers::is_refany_type;
use super::wrappers::{has_destructor, should_emit_wrapper};

/// Go-native type name of a callback arg / return (`Dom` -> `AzDom`).
fn go_native(t: &str) -> String {
    super::ffi_type_name(t)
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
fn arg_go_name(i: usize, t: &str, ir: &CodegenIR) -> String {
    if i == 0 && is_refany_type(t, ir) {
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
fn arg_go_type(t: &str, wrapper_types: &[String], ir: &CodegenIR) -> String {
    if t == "usize" {
        "uint".to_string()
    } else if is_refany_type(t, ir) {
        "any".to_string()
    } else if wrapper_types.iter().any(|w| w.as_str() == t) {
        format!("*{}", t)
    } else {
        "unsafe.Pointer".to_string()
    }
}

/// Does the class have a `log` method (the observability sink a callback
/// info offers)? Such an argument is where binding errors are reported.
fn class_has_log(t: &str, ir: &CodegenIR) -> bool {
    ir.functions_for_class(t).any(|f| {
        matches!(f.kind, FunctionKind::Method | FunctionKind::MethodMut) && f.method_name == "log"
    })
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

/// Number of pointer parameters the invoker of this kind receives after
/// the handle: one per argument plus the out-pointer when there is a
/// return.
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

/// The engine's RefAny type as the IR names it, plus its deep-copy export.
struct RefAnyInfo {
    name: String,
    clone_c_name: Option<String>,
}

fn refany_info(ir: &CodegenIR) -> Option<RefAnyInfo> {
    let s = ir
        .structs
        .iter()
        .find(|s| matches!(s.category, TypeCategory::RefAny))?;
    let clone_c_name = ir
        .functions_for_class(&s.name)
        .find(|f| matches!(f.kind, FunctionKind::DeepCopy))
        .map(|f| f.c_name.clone());
    Some(RefAnyInfo {
        name: s.name.clone(),
        clone_c_name,
    })
}

// ============================================================================
// callbacks_trampolines.go
// ============================================================================

/// Generate the contents of `callbacks_trampolines.go`.
pub fn generate_trampolines(ir: &CodegenIR, config: &CodegenConfig) -> Result<String> {
    let kinds = kind_list(ir);
    let arities = arity_set(&kinds);
    let mut b = CodeBuilder::new(&config.indent);

    b.line("// ============================================================================");
    b.line("// callbacks_trampolines.go - the Go functions libazul calls back into.");
    b.line("// Auto-generated by azul-doc codegen v2 (lang_go). DO NOT EDIT MANUALLY.");
    b.line("// ============================================================================");
    b.line("//");
    b.line("// One function per invoker *arity* (pointer parameters after the uint64");
    b.line("// handle). LoadLibrary wraps each one with purego.NewCallback once and");
    b.line("// registers the resulting C function pointer for every callback kind of");
    b.line("// that arity (callbacks.go). The uintptr result is required by the Windows");
    b.line("// callback ABI and always 0: results travel through the out-pointer.");
    b.blank();
    b.line("package azul");
    b.blank();
    b.line("import \"unsafe\"");
    b.blank();
    for n in &arities {
        let params: Vec<String> = (0..*n).map(|i| format!("p{} unsafe.Pointer", i)).collect();
        let fwd: Vec<String> = (0..*n).map(|i| format!("p{}", i)).collect();
        b.line(&format!(
            "func azGoInvoker{}(handle uint64, {}) uintptr {{",
            n,
            params.join(", ")
        ));
        b.line(&format!("    azGoDispatch(handle, {})", fwd.join(", ")));
        b.line("    return 0");
        b.line("}");
        b.blank();
    }
    b.line("// azGoHostHandleRelease is libazul's host-handle releaser: the last clone of");
    b.line("// a RefAny carrying a Go handle was dropped, so the registry entry goes and");
    b.line("// the Go value becomes collectable.");
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
    let refany = refany_info(ir);
    let mut b = CodeBuilder::new(&config.indent);

    emit_header(&mut b);
    emit_host_invoker_bindings(&mut b, &kinds, &arities, refany.as_ref());
    emit_registry(&mut b);
    emit_string_helpers(&mut b);
    emit_error_reporting(&mut b);
    if let Some(refany) = &refany {
        emit_refany_helpers(&mut b, refany);
    }
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
    b.line("//   1. LoadLibrary registers ONE trampoline per invoker arity");
    b.line("//      (callbacks_trampolines.go, via purego.NewCallback) with");
    b.line("//      AzApp_set<Kind>Invoker, plus a shared releaser via");
    b.line("//      AzApp_setHostHandleReleaser.");
    b.line("//   2. Register<Kind>(fn) stores the Go function in a process-global");
    b.line("//      registry under a fresh uint64 handle and returns the C callback");
    b.line("//      wrapper struct built by Az<Kind>_createFromHostHandleByref(handle) -");
    b.line("//      its cb is a static thunk inside libazul, its ctx a RefAny carrying");
    b.line("//      the handle.");
    b.line("//   3. When the callback fires, libazul's thunk extracts the handle and");
    b.line("//      calls the registered trampoline with POINTER arguments only; the");
    b.line("//      adapter stored in the registry casts them back and calls your Go");
    b.line("//      function. A non-nil result is written through the out-pointer; nil");
    b.line("//      leaves libazul's default for that kind in place.");
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
    b.line("import (");
    b.line("    \"fmt\"");
    b.line("    \"log\"");
    b.line("    \"runtime\"");
    b.line("    \"sync\"");
    b.line("    \"sync/atomic\"");
    b.line("    \"unsafe\"");
    b.blank();
    b.line("    \"github.com/ebitengine/purego\"");
    b.line(")");
    b.blank();
}

/// The purego function values for libazul's host-invoker exports. The
/// invoker setters and the releaser are bound eagerly in `azInitCallbacks`
/// (they must be installed before the first callback can fire); the
/// struct-producing factories bind lazily like every other export.
fn emit_host_invoker_bindings(
    b: &mut CodeBuilder,
    kinds: &[Kind],
    arities: &[usize],
    refany: Option<&RefAnyInfo>,
) {
    b.line("// ============================================================================");
    b.line("// Host-invoker exports of libazul");
    b.line("// ============================================================================");
    b.blank();
    b.line("var libAzApp_setHostHandleReleaser func(uintptr)");
    if let Some(r) = refany {
        let n = go_native(&r.name);
        b.line(&format!("var libAzRefAny_newHostHandleByref func(uint64, *{})", n));
        b.line("var onceAzRefAny_newHostHandleByref sync.Once");
        b.line(&format!("var libAzRefAny_getHostHandle func(*{}) uint64", n));
        b.line("var onceAzRefAny_getHostHandle sync.Once");
    }
    for k in kinds {
        b.line(&format!("var libAzApp_set{w}Invoker func(uintptr)", w = k.wrapper));
        b.line(&format!(
            "var libAz{w}_createFromHostHandleByref func(uint64, *Az{w})",
            w = k.wrapper
        ));
        b.line(&format!("var onceAz{w}_createFromHostHandleByref sync.Once", w = k.wrapper));
    }
    b.blank();
    for k in kinds {
        b.line(&format!(
            "func azGoCreate{w}(id uint64, out *Az{w}) {{",
            w = k.wrapper
        ));
        b.line(&format!(
            "    onceAz{w}_createFromHostHandleByref.Do(func() {{ azRegister(&libAz{w}_createFromHostHandleByref, \"Az{w}_createFromHostHandleByref\") }})",
            w = k.wrapper
        ));
        b.line(&format!("    libAz{w}_createFromHostHandleByref(id, out)", w = k.wrapper));
        b.line("}");
        b.blank();
    }
    b.line("// azInitCallbacks installs the releaser and the per-arity trampolines in");
    b.line("// libazul. LoadLibrary calls it exactly once, right after azLib is set.");
    b.line("func azInitCallbacks() {");
    b.indent();
    b.line("azRegister(&libAzApp_setHostHandleReleaser, \"AzApp_setHostHandleReleaser\")");
    b.line("libAzApp_setHostHandleReleaser(purego.NewCallback(azGoHostHandleRelease))");
    for n in arities {
        b.line(&format!("invoker{n} := purego.NewCallback(azGoInvoker{n})", n = n));
    }
    for k in kinds {
        b.line(&format!(
            "azRegister(&libAzApp_set{w}Invoker, \"AzApp_set{w}Invoker\")",
            w = k.wrapper
        ));
        b.line(&format!(
            "libAzApp_set{w}Invoker(invoker{n})",
            w = k.wrapper,
            n = arity(k)
        ));
    }
    b.dedent();
    b.line("}");
    b.blank();
}

fn emit_registry(b: &mut CodeBuilder) {
    b.line("// ============================================================================");
    b.line("// Handle registry");
    b.line("// ============================================================================");
    b.blank();
    b.line("// azGoAdapter is the uniform shape every registered callback is stored as:");
    b.line("// the per-kind Register function wraps the user's typed Go function in a");
    b.line("// closure that casts the raw pointer arguments back to their Go-native types.");
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
    b.line("func Str(s string) *String {");
    b.line("    b := []byte(s)");
    b.line("    ptr := &azGoEmptyByte");
    b.line("    if len(b) > 0 {");
    b.line("        ptr = &b[0]");
    b.line("    }");
    b.line("    raw := AzString_fromUtf8(ptr, uintptr(len(b)))");
    b.line("    ret := &String{ inner: &raw }");
    b.line("    runtime.SetFinalizer(ret, func(x *String) { x.Close() })");
    b.line("    return ret");
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

    // A managed `*String` had no way to read its own text. The obvious name,
    // `String()`, is taken: api.json gives the class `Debug`, so the wrapper
    // generator emits a `String()` returning `AzString_toDbgString`, and two
    // methods of that name in one package do not compile.
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

fn emit_error_reporting(b: &mut CodeBuilder) {
    b.line("// ============================================================================");
    b.line("// Error reporting for binding failures");
    b.line("// ============================================================================");
    b.blank();
    b.line("// azGoLogger is satisfied by every callback info that exposes libazul's");
    b.line("// log sink (CallbackInfo.Log): binding errors raised inside a callback are");
    b.line("// routed there so they reach the observability pipeline.");
    b.line("type azGoLogger interface {");
    b.line("    Log(AppLogLevel, *String)");
    b.line("}");
    b.blank();
    b.line("// azGoReportError sends a binding error to the first of `sinks` that is a");
    b.line("// libazul log sink, else to the standard logger.");
    b.line("func azGoReportError(msg string, sinks ...any) {");
    b.line("    for _, s := range sinks {");
    b.line("        if l, ok := s.(azGoLogger); ok {");
    b.line("            l.Log(AppLogLevel_Error, Str(msg))");
    b.line("            return");
    b.line("        }");
    b.line("    }");
    b.line("    log.Print(msg)");
    b.line("}");
    b.blank();
}

fn emit_refany_helpers(b: &mut CodeBuilder, refany: &RefAnyInfo) {
    let w = super::sanitize_identifier(&refany.name);
    let n = go_native(&refany.name);
    b.line("// ============================================================================");
    b.line(&format!("// {w} wrap/get (arbitrary Go values as libazul app data)"));
    b.line("// ============================================================================");
    b.blank();
    b.line(&format!("// {w}Wrap stores an arbitrary Go value in the handle registry and wraps"));
    b.line(&format!("// the handle in a {w}. The value stays reachable until libazul drops the"));
    b.line(&format!("// last clone of the {w}, at which point the releaser removes the registry"));
    b.line("// entry and the Go GC may collect it. Store a POINTER (e.g. *MyModel) if");
    b.line(&format!("// callbacks should observe mutations across invocations. A *{w} is"));
    b.line("// returned as is.");
    b.line(&format!("func {w}Wrap(value any) *{w} {{"));
    b.line(&format!("    if r, ok := value.(*{w}); ok {{"));
    b.line("        return r");
    b.line("    }");
    b.line("    id := azGoNewHandle(value)");
    b.line(&format!("    var inner {n}"));
    b.line("    onceAzRefAny_newHostHandleByref.Do(func() { azRegister(&libAzRefAny_newHostHandleByref, \"AzRefAny_newHostHandleByref\") })");
    b.line("    libAzRefAny_newHostHandleByref(id, &inner)");
    b.line(&format!("    self := &{w}{{ inner: &inner }}"));
    b.line(&format!("    runtime.SetFinalizer(self, func(x *{w}) {{ x.Close() }})"));
    b.line("    return self");
    b.line("}");
    b.blank();
    b.line(&format!("// {w}Get recovers the Go value previously wrapped via {w}Wrap."));
    b.line(&format!("// Returns (nil, false) if the {w} is not a host handle (e.g. it was"));
    b.line("// created natively) or the handle has already been released.");
    b.line(&format!("func {w}Get(ref *{w}) (any, bool) {{"));
    b.line("    if ref == nil || ref.inner == nil {");
    b.line("        return nil, false");
    b.line("    }");
    b.line("    onceAzRefAny_getHostHandle.Do(func() { azRegister(&libAzRefAny_getHostHandle, \"AzRefAny_getHostHandle\") })");
    b.line("    id := libAzRefAny_getHostHandle(ref.inner)");
    b.line("    if id == 0 {");
    b.line("        return nil, false");
    b.line("    }");
    b.line("    return azGoHandles.Load(id)");
    b.line("}");
    b.blank();
    b.line(&format!("// azGoRefAnyOwned turns a `data any` argument into the {n} a consuming"));
    b.line(&format!("// (owned) libazul parameter expects. A *{w} keeps its own reference: the"));
    b.line("// callee receives a clone, so the caller's wrapper and its finalizer stay");
    b.line("// valid. Any other Go value is wrapped into a fresh host handle whose single");
    b.line("// reference moves to the callee, leaving no wrapper behind that could");
    b.line("// release it a second time.");
    b.line(&format!("func azGoRefAnyOwned(value any) {n} {{"));
    b.line(&format!("    if r, ok := value.(*{w}); ok {{"));
    b.line("        if r.inner == nil {");
    b.line(&format!("            panic(\"azul: {w} used after Close() or Raw()\")"));
    b.line("        }");
    match &refany.clone_c_name {
        Some(clone) => b.line(&format!("        return {clone}(r.inner)")),
        None => b.line("        return r.Raw()"),
    }
    b.line("    }");
    b.line(&format!("    return {w}Wrap(value).Raw()"));
    b.line("}");
    b.blank();
    b.line("// Bind adapts a typed Go callback `func(*T, Ctx) Ret` to the `func(any, Ctx)");
    b.line("// any` shape the Register<Kind> functions and smart setters take, downcasting");
    b.line(&format!("// the {w} payload to *T. If the payload is not a *T the failure is logged"));
    b.line("// (through the callback info's Log when it has one, else the standard");
    b.line("// logger) and nil is returned, which makes the adapter keep libazul's");
    b.line("// default for that callback kind (Update_DoNothing, an empty body, ...) so");
    b.line("// the app keeps running.");
    b.line("func Bind[T any, Ctx any, Ret any](cb func(*T, Ctx) Ret) func(any, Ctx) any {");
    b.line("    return func(data any, ctx Ctx) any {");
    b.line("        model, ok := data.(*T)");
    b.line("        if !ok {");
    b.line("            azGoReportError(fmt.Sprintf(\"azul.Bind: type assertion failed, expected %T, got %T\", new(T), data), any(ctx))");
    b.line("            return nil");
    b.line("        }");
    b.line("        return cb(model, ctx)");
    b.line("    }");
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
            .map(|(i, t)| arg_go_name(i, t, ir))
            .collect();
        let mut params: Vec<String> = names
            .iter()
            .zip(&k.arg_types)
            .map(|(n, t)| format!("{} {}", n, arg_go_type(t, wrapper_types, ir)))
            .collect();
        if matches!(rk, RetKind::OutParam(_)) {
            params.push("out unsafe.Pointer".to_string());
        }
        let sig = params.join(", ");
        // Where binding errors go: the wrapper-typed arguments whose class
        // offers a `log` method (the callback info). Empty -> standard logger.
        let sinks: Vec<String> = names
            .iter()
            .zip(&k.arg_types)
            .filter(|(_, t)| wrapper_types.iter().any(|w| w == *t) && class_has_log(t, ir))
            .map(|(n, _)| format!(", {}", n))
            .collect();
        let sinks = sinks.concat();
        // The out-pointer follows the arguments in the invoker's pointer array.
        let out_index = k.arg_types.len();

        b.line(&format!(
            "// {}Func is the Go signature for {} callbacks. Pointer",
            k.wrapper, k.wrapper
        ));
        b.line("// arguments are borrowed views into libazul's callback frame: valid only");
        b.line("// for the duration of the call - do not retain or Close them.");
        match &rk {
            RetKind::Enum(r) => {
                b.line(&format!(
                    "// Return {} (or nil to keep libazul's default for this kind).",
                    go_native(r)
                ));
            }
            RetKind::Wrapper(r) => {
                b.line(&format!(
                    "// Return *{} or a {} value (or nil to keep libazul's default for this kind).",
                    r,
                    go_native(r)
                ));
            }
            RetKind::OutParam(r) => {
                b.line(&format!(
                    "// The result must be written through `out` (*{}); no Go",
                    go_native(r)
                ));
                b.line("// wrapper type exists for this return type yet.");
            }
            RetKind::Void => {}
        }
        match &rk {
            RetKind::Enum(_) | RetKind::Wrapper(_) => {
                b.line(&format!("type {}Func func({}) any", k.wrapper, sig))
            }
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
                b.line(&format!("        {} := uint(*(*uintptr)(args[{}]))", nm, i));
            } else if is_refany_type(t, ir) {
                let w = super::sanitize_identifier(t);
                b.line(&format!(
                    "        {nm}_wrap := &{w}{{ inner: (*{n})(args[{i}]), borrowed: true }}",
                    n = go_native(t)
                ));
                b.line(&format!("        {nm}, _ := {w}Get({nm}_wrap)"));
            } else if wrapper_types.iter().any(|w| w == t) {
                let has_del = has_destructor(t, ir);
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
        match &rk {
            RetKind::Void => b.line(&format!("        fn({})", call_args)),
            RetKind::OutParam(_) => {
                b.line(&format!("        fn({}, args[{}])", call_args, out_index))
            }
            RetKind::Enum(r) => {
                let c = go_native(r);
                b.line(&format!("        ret := fn({})", call_args));
                b.line("        if ret == nil {");
                b.line("            return // keep libazul's default");
                b.line("        }");
                b.line(&format!("        if v, ok := ret.({c}); ok {{"));
                b.line(&format!("            *(*{c})(args[{out_index}]) = v"));
                b.line("        } else {");
                b.line(&format!(
                    "            azGoReportError(fmt.Sprintf(\"azul: {w} callback returned %T, expected {c} or nil\", ret){sinks})",
                    w = k.wrapper
                ));
                b.line("        }");
            }
            RetKind::Wrapper(r) => {
                let has_del = has_destructor(r, ir);
                let c = go_native(r);
                b.line(&format!("        ret := fn({})", call_args));
                b.line("        if ret == nil {");
                b.line("            return // keep libazul's default");
                b.line("        }");
                // The raw Go-native value is accepted too: it is what a
                // caller builds when the type has no constructor
                // (`AzOnTextInputReturn{...}`) - ownership moves to libazul.
                b.line(&format!("        if raw, ok := ret.({c}); ok {{"));
                b.line(&format!("            *(*{c})(args[{out_index}]) = raw"));
                b.line("            return");
                b.line("        }");
                b.line(&format!("        v, ok := ret.(*{r})"));
                b.line("        if !ok {");
                b.line(&format!(
                    "            azGoReportError(fmt.Sprintf(\"azul: {w} callback returned %T, expected *{r}, {c} or nil\", ret){sinks})",
                    w = k.wrapper
                ));
                b.line("            return");
                b.line("        }");
                if has_del {
                    b.line("        if v == nil || v.inner == nil {");
                    b.line("            return // typed nil / consumed wrapper: keep libazul's default");
                    b.line("        }");
                    b.line(&format!("        *(*{c})(args[{out_index}]) = *v.inner"));
                    b.line("        v.inner = nil");
                } else {
                    b.line("        if v == nil {");
                    b.line("            return // typed nil: keep libazul's default");
                    b.line("        }");
                    b.line(&format!("        *(*{c})(args[{out_index}]) = v.inner"));
                }
                b.line("        runtime.SetFinalizer(v, nil)");
            }
        }
        b.line("    }))");
        b.line(&format!("    var out Az{w}", w = k.wrapper));
        b.line(&format!("    azGoCreate{w}(id, &out)", w = k.wrapper));
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
    // Smart layout factories: for every struct matching the shared
    // `layout_callback_factory_info` shape (a `create(<callback kind>)`
    // constructor + a `_default`), emit `<Class>Create(fn)` that builds the
    // default value and splices the registered callback into the field the
    // IR scan located. wrappers.rs suppresses the raw constructor of the
    // same name (see `is_layout_factory_constructor`).
    for s in ir.structs.iter().filter(|s| should_emit_wrapper(s, ir, config)) {
        let Some(info) = layout_callback_factory_info(s, ir) else {
            continue;
        };
        let wrapper_class = super::sanitize_identifier(&info.class_name);
        let register_fn = format!("Register{}", info.callback_wrapper);
        let fn_name = format!("{}Create", wrapper_class);
        let has_del = has_destructor(&info.class_name, ir);
        b.line(&format!("// {} builds a {} whose layout callback", fn_name, wrapper_class));
        b.line("// is the given Go function (host-invoker registered, ctx-preserving).");
        b.line(&format!(
            "func {}(fn {}Func) *{} {{",
            fn_name, info.callback_wrapper, wrapper_class
        ));
        b.line(&format!("    val := {}()", info.default_c_name));
        let mut field_path = "val".to_string();
        for part in &info.field_path {
            field_path.push('.');
            field_path.push_str(&super::types::go_field_name(part));
        }
        b.line(&format!("    {} = {}(fn)", field_path, register_fn));
        if has_del {
            b.line(&format!("    self := &{}{{ inner: &val }}", wrapper_class));
            b.line(&format!(
                "    runtime.SetFinalizer(self, func(x *{}) {{ x.Close() }})",
                wrapper_class
            ));
        } else {
            b.line(&format!("    self := &{}{{ inner: val }}", wrapper_class));
        }
        b.line("    return self");
        b.line("}");
        b.blank();
    }

    // Per-widget On<Event> smart setters: for every instance method
    // `set_on_x(self, data: RefAny, cb: <Kind>)` whose kind is in the
    // host-invoker allowlist, emit `On<X>(data any, fn <Kind>Func)`.
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
            if !is_refany_type(&f.args[1].type_name, ir) {
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
            b.line("// the model the callback receives (a *RefAny is cloned, any other");
            b.line("// value wrapped); the caller keeps ownership of what it passed.");
            b.line(&format!(
                "func (self *{}) {}(data any, fn {}Func) {{",
                go_name, method, cb_ty
            ));
            let self_expr = if has_destructor(&s.name, ir) {
                "self.inner"
            } else {
                "&self.inner"
            };
            b.line(&format!(
                "    {}({}, azGoRefAnyOwned(data), Register{}(fn))",
                f.c_name, self_expr, cb_ty
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
    b.line("// libazul parameter (AddChild, Run, ...). Clone() the wrapper first");
    b.line("// if you still need it afterwards. A wrapper that merely borrows its value");
    b.line("// (a callback argument) hands out a clone instead and stays usable.");
    b.blank();
    for t in wrapper_types {
        let ffi = go_native(t);
        b.line(&format!(
            "// Raw returns the underlying {} value, transferring ownership to",
            ffi
        ));
        b.line("// the caller (the wrapper's finalizer, if any, is disarmed).");
        b.line(&format!("func (self *{t}) Raw() {ffi} {{"));
        if has_destructor(t, ir) {
            let clone = ir
                .functions_for_class(t)
                .find(|f| matches!(f.kind, FunctionKind::DeepCopy))
                .map(|f| f.c_name.clone());
            b.line("    if self.borrowed {");
            match clone {
                Some(clone) => b.line(&format!("        return {clone}(self.inner)")),
                None => b.line(&format!(
                    "        panic(\"azul: {t}.Raw(): cannot take ownership of a borrowed value (no clone)\")"
                )),
            }
            b.line("    }");
            b.line("    runtime.SetFinalizer(self, nil)");
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
