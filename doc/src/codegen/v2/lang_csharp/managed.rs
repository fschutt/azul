//! C# managed-FFI runtime (host-invoker pattern): the `HostInvoker` class.
//!
//! libazul dispatches every managed callback through ONE process-global
//! "invoker" per callback kind (`azul_core::host_invoker`): the engine's
//! static thunk receives the C-ABI arguments by value, resolves the host
//! handle stored in the callback's `RefAny` ctx and calls the registered
//! invoker with `(handle id, *const arg, …, *mut out)`. The thunk keeps
//! ownership of every argument (it drops them after the invoker returns)
//! and pre-fills `out` with the kind's default return, so an invoker that
//! writes nothing yields `Update::DoNothing` / `Dom::default()`.
//!
//! This module emits the C# half:
//!
//! 1. `NativeMethodsManaged` — `[DllImport]`s for the host-invoker exports
//!    (`AzApp_set<X>Invoker`, `Az<X>_createFromHostHandle`,
//!    `AzRefAny_newHostHandle`, `AzRefAny_getHostHandle`,
//!    `AzApp_setHostHandleReleaser`). These are not part of api.json / azul.h.
//! 2. `HostInvoker` — the public runtime:
//!    * `<X>InvokerDelegate` — the thunk signature `(ulong id, IntPtr arg…[, IntPtr outPtr])`.
//!      One static instance per kind is pinned for the process lifetime and dispatches by id;
//!      every registered handle IS an instance of this delegate, so dispatch is a direct call.
//!    * `<X>WithData<T>` — the typed delegate users write, e.g.
//!      `Update OnClick(MyModel data, CallbackInfo info)`; emitted for every kind whose shape
//!      [`typed_delegate_info`] can plumb (wrappers.rs uses the same predicate before it emits
//!      the `Button.OnClick<T>` / `WindowCreateOptions.Create<T>` builders).
//!    * `Register<X><T>(typed)` / `Register<X>(raw)` — store the delegate under a fresh id and
//!      return the `Az<X>` wrapper struct whose ctx carries that id.
//!    * `RefanyCreate` / `RefanyGet` / `RefanyWrap` — user data as host-handle `RefAny`.
//!
//! Users normally never call `Register*` themselves: the wrapper classes expose
//! `Button.OnClick(model, OnClick)`, `WindowCreateOptions.Create<Model>(Layout)` and
//! `App.Create(model, config)`.

use super::{
    super::{
        generator::CodeBuilder,
        ir::{CallbackTypedefDef, CodegenIR, MonomorphizedKind},
        managed_host_invoker::{has_return, host_invoker_kinds, wrapper_name},
        managed_lang_helpers::{has_wrapper_class, is_refany_type},
    },
    ffi_type_name, map_type_to_csharp, user_enum_type_name, DLL_NAME,
};

// ============================================================================
// Typed-delegate classification (shared by managed.rs and wrappers.rs)
// ============================================================================

/// How one callback argument (after the leading `RefAny` data slot) is
/// surfaced in the typed `<X>WithData<T>` delegate. The thunk hands every
/// argument to the invoker as a pointer to an engine-owned value.
pub(super) enum TypedArg {
    /// The type has a wrapper class: surfaced as a *borrowed* wrapper
    /// (`X.__Borrow(Marshal.PtrToStructure<AzX>(p))`) so its methods are
    /// reachable but neither `Dispose()` nor the finalizer ever frees the
    /// engine's bytes.
    Wrapper(String),
    /// Unit enum (`Update`, `TextInputValid`, …): `(E)Marshal.ReadInt32(p)`.
    UnitEnum(String),
    /// `bool` is 1 byte on the C side but `Marshal` treats it as a 4-byte
    /// BOOL: `Marshal.ReadByte(p) != 0`.
    Bool,
    /// Any other blittable value (POD struct, tagged union, primitive):
    /// `Marshal.PtrToStructure<cs>(p)`.
    Value(String),
    /// Unmapped type: passed through as the raw `IntPtr`.
    RawIntPtr,
}

/// How the typed delegate's return value is written into `outPtr`.
pub(super) enum TypedRet {
    Void,
    /// Wrapper class: `Raw` is written by value and the wrapper is marked
    /// consumed (ownership moved to the engine).
    Wrapper(String),
    /// `Marshal.WriteInt32(outPtr, (int)result)`.
    UnitEnum(String),
    /// `Marshal.WriteByte(outPtr, result ? 1 : 0)`.
    Bool,
    /// `Marshal.StructureToPtr(result, outPtr, false)` (POD struct, tagged
    /// union, primitive).
    Value(String),
}

impl TypedRet {
    /// The C# return type of the typed delegate.
    pub(super) fn cs_type(&self) -> &str {
        match self {
            TypedRet::Void => "void",
            TypedRet::Wrapper(t) | TypedRet::UnitEnum(t) | TypedRet::Value(t) => t,
            TypedRet::Bool => "bool",
        }
    }
}

/// Everything needed to emit `<X>WithData<T>` and its `Register<X><T>`.
pub(super) struct TypedDelegateInfo {
    /// Callback wrapper name (`"ButtonOnClickCallback"`).
    pub wrapper: String,
    /// The arguments after the data slot, with their C# parameter names.
    pub args: Vec<(TypedArg, String)>,
    pub ret: TypedRet,
}

impl TypedDelegateInfo {
    /// The delegate type name (`HostInvoker.<X>WithData<T>` without the
    /// class prefix).
    pub(super) fn delegate_name(&self) -> String {
        format!("{}WithData<T>", self.wrapper)
    }
}

/// Unit-enum C# name for `t` (plain unit enums and monomorphized
/// `SimpleEnum` aliases are both emitted unprefixed, see mod.rs).
fn unit_enum_name(t: &str, ir: &CodegenIR) -> Option<String> {
    if ir.find_enum(t).is_some_and(|e| !e.is_union) {
        return Some(user_enum_type_name(t));
    }
    if let Some(ta) = ir.find_type_alias(t) {
        if let Some(mono) = &ta.monomorphized_def {
            if matches!(mono.kind, MonomorphizedKind::SimpleEnum { .. }) {
                return Some(user_enum_type_name(t));
            }
        }
    }
    None
}

/// Blittable C# type that `Marshal.PtrToStructure` / `StructureToPtr` can
/// move for `t`, or `None` when the binding has no value form for it
/// (callback typedefs, recursive structs, unknown names).
fn blittable_cs_type(t: &str, ir: &CodegenIR) -> Option<String> {
    if ir.callback_typedefs.iter().any(|c| c.name == t) {
        return None;
    }
    let cs = map_type_to_csharp(t, ir);
    if cs == "void" {
        return None;
    }
    if cs == "IntPtr" {
        // `map_type_to_csharp` collapses raw pointers, `isize` AND
        // everything it cannot resolve to `IntPtr`; only the first two
        // are genuinely pointer-sized values the thunk points at.
        let is_pointer_value = t.starts_with('*') || t.starts_with('&') || t == "isize";
        if !is_pointer_value {
            return None;
        }
    }
    Some(cs)
}

fn classify_arg(t: &str, ir: &CodegenIR) -> TypedArg {
    if has_wrapper_class(t, ir) {
        return TypedArg::Wrapper(t.to_string());
    }
    if let Some(e) = unit_enum_name(t, ir) {
        return TypedArg::UnitEnum(e);
    }
    if t == "bool" {
        return TypedArg::Bool;
    }
    match blittable_cs_type(t, ir) {
        Some(cs) => TypedArg::Value(cs),
        None => TypedArg::RawIntPtr,
    }
}

fn classify_ret(rt: Option<&str>, ir: &CodegenIR) -> Option<TypedRet> {
    let rt = match rt.map(str::trim) {
        None | Some("void") | Some("()") => return Some(TypedRet::Void),
        Some(rt) => rt,
    };
    if has_wrapper_class(rt, ir) {
        return Some(TypedRet::Wrapper(rt.to_string()));
    }
    if let Some(e) = unit_enum_name(rt, ir) {
        return Some(TypedRet::UnitEnum(e));
    }
    if rt == "bool" {
        return Some(TypedRet::Bool);
    }
    blittable_cs_type(rt, ir).map(TypedRet::Value)
}

/// THE predicate for "a typed `<X>WithData<T>` delegate exists for this
/// callback kind". Both the delegate emitter below and the smart
/// builders in wrappers.rs consult it, so the two can never drift apart.
///
/// Rules (all IR-derived): the first argument must be the engine's
/// `RefAny` (the data slot, see [`is_refany_type`]); every further
/// argument is classified by [`classify_arg`] (never fails: unmapped
/// types stay `IntPtr`); the return type must be void, a wrapper class,
/// a unit enum, `bool` or a blittable value — anything else has no
/// out-pointer write rule and gets no typed delegate.
pub(super) fn typed_delegate_info(
    cb: &CallbackTypedefDef,
    ir: &CodegenIR,
) -> Option<TypedDelegateInfo> {
    let first = cb.args.first()?;
    if !is_refany_type(&first.type_name, ir) {
        return None;
    }
    let args = cb
        .args
        .iter()
        .enumerate()
        .skip(1)
        .map(|(i, a)| (classify_arg(a.type_name.trim(), ir), arg_name(a, i)))
        .collect();
    let ret = classify_ret(cb.return_type.as_deref(), ir)?;
    Some(TypedDelegateInfo {
        wrapper: wrapper_name(cb).to_string(),
        args,
        ret,
    })
}

/// [`typed_delegate_info`] looked up by callback wrapper name
/// (`"ButtonOnClickCallback"`), as wrappers.rs sees it through
/// `smart_callback_setter_info` / `layout_callback_factory_info`.
pub(super) fn typed_delegate_info_for_kind(
    kind: &str,
    ir: &CodegenIR,
) -> Option<TypedDelegateInfo> {
    host_invoker_kinds(ir)
        .find(|cb| wrapper_name(cb) == kind)
        .and_then(|cb| typed_delegate_info(cb, ir))
}

fn arg_name(a: &super::super::ir::FunctionArg, i: usize) -> String {
    if a.name.is_empty() {
        format!("arg{}", i)
    } else {
        a.name.clone()
    }
}

/// `(ulong id, IntPtr arg0, …[, IntPtr outPtr])` — the invoker signature
/// shared by the delegate type, the per-kind static invoker and the raw
/// lambda inside `Register<X><T>`.
fn invoker_params(cb: &CallbackTypedefDef) -> Vec<String> {
    let mut params = vec!["ulong id".to_string()];
    for (i, a) in cb.args.iter().enumerate() {
        params.push(format!("IntPtr {}", arg_name(a, i)));
    }
    if has_return(cb) {
        params.push("IntPtr outPtr".to_string());
    }
    params
}

fn invoker_arg_names(cb: &CallbackTypedefDef) -> Vec<String> {
    invoker_params(cb)
        .iter()
        .map(|p| p.rsplit(' ').next().unwrap_or(p).to_string())
        .collect()
}

// ============================================================================
// NativeMethodsManaged
// ============================================================================

/// Emit a separate `NativeMethodsManaged` class with `[DllImport]`
/// declarations for the host-invoker exports. Lives next to `NativeMethods`
/// rather than inside it because we don't want to surgery into
/// `functions::generate_native_methods`.
///
/// Inserted from `mod.rs` AFTER `functions::generate_native_methods`.
pub fn emit_native_method_imports(builder: &mut CodeBuilder, ir: &CodegenIR) {
    builder.line("// --------------------------------------------------------------------------");
    builder.line("// NativeMethodsManaged: P/Invoke imports for the host-invoker C-ABI exports.");
    builder.line("// (Kept separate from NativeMethods so the regular function-binding emitter");
    builder.line("// stays linear; same DLL, same calling convention.)");
    builder.line("// --------------------------------------------------------------------------");
    builder.blank();

    builder.line("internal static class NativeMethodsManaged");
    builder.line("{");
    builder.indent();
    builder.line(&format!("public const string DllName = \"{}\";", DLL_NAME));
    builder.blank();

    builder.line("[DllImport(DllName, CallingConvention = CallingConvention.Cdecl)]");
    builder.line("public static extern void AzApp_setHostHandleReleaser(IntPtr fn);");
    builder.blank();
    builder.line("[DllImport(DllName, CallingConvention = CallingConvention.Cdecl)]");
    builder.line("public static extern AzRefAny AzRefAny_newHostHandle(ulong id);");
    builder.blank();
    builder.line("[DllImport(DllName, CallingConvention = CallingConvention.Cdecl)]");
    builder.line("public static extern ulong AzRefAny_getHostHandle(IntPtr refanyPtr);");
    builder.blank();

    for cb in host_invoker_kinds(ir) {
        let wrapper = wrapper_name(cb);
        builder.line("[DllImport(DllName, CallingConvention = CallingConvention.Cdecl)]");
        builder.line(&format!(
            "public static extern void AzApp_set{}Invoker(IntPtr fn);",
            wrapper
        ));
        builder.blank();
        builder.line("[DllImport(DllName, CallingConvention = CallingConvention.Cdecl)]");
        builder.line(&format!(
            "public static extern Az{w} Az{w}_createFromHostHandle(ulong id);",
            w = wrapper
        ));
        builder.blank();
    }

    builder.dedent();
    builder.line("}");
    builder.blank();
}

// ============================================================================
// HostInvoker
// ============================================================================

/// Emit the public `Azul.HostInvoker` static class. Inserted from
/// `mod.rs` AFTER `wrappers::generate_wrappers` (so user-facing wrappers
/// are visible) but inside `namespace Azul { ... }`.
pub fn emit_host_invoker_class(builder: &mut CodeBuilder, ir: &CodegenIR) {
    builder.blank();
    builder.line("// ────────────────────────────────────────────────────────────────");
    builder.line("// Managed-FFI runtime: host-invoker public surface.");
    builder.line("//");
    builder.line("// Every registered callback is stored as a <Kind>InvokerDelegate under");
    builder.line("// a process-global id; libazul's static thunk calls the ONE pinned");
    builder.line("// per-kind invoker below, which looks the id up and calls the stored");
    builder.line("// delegate directly. User data lives in the same table (RefanyCreate);");
    builder.line("// the engine's RefAny destructor calls back through the registered");
    builder.line("// releaser so an entry is dropped when its last clone is collected.");
    builder.line("// ────────────────────────────────────────────────────────────────");
    builder.blank();

    builder.line("public static class HostInvoker");
    builder.line("{");
    builder.indent();

    // Delegate types. Every one of them is handed to
    // `GetFunctionPointerForDelegate`, so the calling convention must be
    // explicit (the default is StdCall on win-x86).
    builder.line("[UnmanagedFunctionPointer(CallingConvention.Cdecl)]");
    builder.line("public delegate void HostHandleReleaserDelegate(ulong id);");
    for cb in host_invoker_kinds(ir) {
        builder.line("/// <summary>Raw invoker signature for ");
        builder.line(&format!(
            "/// {}: (handle id, pointer per C-ABI argument{}). Prefer the typed",
            wrapper_name(cb),
            if has_return(cb) {
                ", out-pointer for the return value"
            } else {
                ""
            }
        ));
        builder.line(&format!(
            "/// `{}WithData&lt;T&gt;` unless you need the raw pointers.</summary>",
            wrapper_name(cb)
        ));
        builder.line("[UnmanagedFunctionPointer(CallingConvention.Cdecl)]");
        builder.line(&format!(
            "public delegate void {}InvokerDelegate({});",
            wrapper_name(cb),
            invoker_params(cb).join(", ")
        ));
    }
    builder.blank();

    // Storage
    builder.line(
        "private static readonly System.Collections.Generic.Dictionary<ulong, object> _handles = \
         new();",
    );
    builder.line("private static ulong _nextHandleId = 0;");
    builder.line(
        "private static readonly System.Collections.Generic.List<Delegate> _livePins = new();",
    );
    builder.line("private static readonly object _initLock = new();");
    builder.line("private static volatile bool _initialized = false;");
    builder.blank();

    // __Pin: keep a delegate that was marshalled to a native function
    // pointer alive for the process lifetime (P/Invoke does not root it;
    // the GC would otherwise collect it while libazul still holds the
    // pointer).
    builder.line("/// <summary>Internal: root a delegate whose function pointer was handed to");
    builder.line("/// native code, for the lifetime of the process.</summary>");
    builder.line("internal static void __Pin(Delegate fn)");
    builder.line("{");
    builder.indent();
    builder.line("if (fn == null) return;");
    builder.line("lock (_livePins) { _livePins.Add(fn); }");
    builder.dedent();
    builder.line("}");
    builder.blank();

    // EnsureInitialized
    builder.line("private static void EnsureInitialized()");
    builder.line("{");
    builder.indent();
    builder.line("if (_initialized) return;");
    builder.line("lock (_initLock)");
    builder.line("{");
    builder.indent();
    builder.line("if (_initialized) return;");
    builder.blank();

    builder.line("// Releaser");
    builder.line("HostHandleReleaserDelegate releaser = (ulong id) =>");
    builder.line("{");
    builder.indent();
    builder.line("lock (_handles) { _handles.Remove(id); }");
    builder.dedent();
    builder.line("};");
    builder.line("__Pin(releaser);");
    builder.line(
        "NativeMethodsManaged.AzApp_setHostHandleReleaser(System.Runtime.InteropServices.Marshal.\
         GetFunctionPointerForDelegate(releaser));",
    );
    builder.blank();

    for cb in host_invoker_kinds(ir) {
        emit_per_kind_invoker_init(builder, cb);
    }

    // Published last: a second thread racing past the outer check must
    // only see `true` once every invoker is registered.
    builder.line("_initialized = true;");
    builder.dedent();
    builder.line("}");
    builder.dedent();
    builder.line("}");
    builder.blank();

    // Store a handle under a fresh id.
    builder.line("private static ulong StoreHandle(object value)");
    builder.line("{");
    builder.indent();
    builder.line("EnsureInitialized();");
    builder.line("lock (_handles)");
    builder.line("{");
    builder.indent();
    builder.line("_nextHandleId++;");
    builder.line("_handles[_nextHandleId] = value;");
    builder.line("return _nextHandleId;");
    builder.dedent();
    builder.line("}");
    builder.dedent();
    builder.line("}");
    builder.blank();

    // Per-kind Register methods: raw + typed.
    for cb in host_invoker_kinds(ir) {
        emit_register_raw(builder, cb);
        if let Some(info) = typed_delegate_info(cb, ir) {
            emit_typed_delegate_and_register(builder, cb, &info);
        }
    }

    // RefanyCreate / RefanyGet
    builder.line("/// <summary>");
    builder.line("/// Wrap an arbitrary managed object in an AzRefAny held alive by the");
    builder.line("/// framework's refcount.");
    builder.line("/// </summary>");
    builder.line("public static AzRefAny RefanyCreate(object value)");
    builder.line("{");
    builder.indent();
    builder.line("return NativeMethodsManaged.AzRefAny_newHostHandle(StoreHandle(value));");
    builder.dedent();
    builder.line("}");
    builder.blank();

    builder.line("/// <summary>");
    builder.line("/// Recover the managed object previously wrapped via RefanyCreate.");
    builder.line("/// Returns null if the RefAny is not a host-handle RefAny.");
    builder.line("/// </summary>");
    builder.line("public static object RefanyGet(IntPtr refanyPtr)");
    builder.line("{");
    builder.indent();
    builder.line("ulong id = NativeMethodsManaged.AzRefAny_getHostHandle(refanyPtr);");
    builder.line("if (id == 0) return null;");
    builder.line("lock (_handles)");
    builder.line("{");
    builder.indent();
    builder.line("return _handles.TryGetValue(id, out var v) ? v : null;");
    builder.dedent();
    builder.line("}");
    builder.dedent();
    builder.line("}");
    builder.blank();

    builder.line("/// <summary>");
    builder.line("/// Wrap an arbitrary managed object in a `RefAny` wrapper.");
    builder.line("/// Convenience over `RefanyCreate(object)` which returns the raw");
    builder.line("/// `AzRefAny` FFI struct.");
    builder.line("/// </summary>");
    builder.line("public static RefAny RefanyWrap(object value)");
    builder.line("{");
    builder.indent();
    builder.line("return new RefAny(RefanyCreate(value));");
    builder.dedent();
    builder.line("}");

    builder.dedent();
    builder.line("}");
    builder.blank();
}

/// The per-kind static invoker: looks the handle up and calls it
/// directly. Every handle registered through `Register<X>` IS a
/// `<X>InvokerDelegate`, so no reflection is involved; a user exception
/// is logged (the thunk then returns the kind's default) rather than
/// unwinding into native frames.
fn emit_per_kind_invoker_init(builder: &mut CodeBuilder, cb: &CallbackTypedefDef) {
    let wrapper = wrapper_name(cb);
    let var = format!("{}Invoker", lower_first(wrapper));
    let args = invoker_arg_names(cb);

    builder.line(&format!("// {} invoker", wrapper));
    builder.line(&format!(
        "{}InvokerDelegate {} = ({}) =>",
        wrapper,
        var,
        invoker_params(cb).join(", ")
    ));
    builder.line("{");
    builder.indent();
    builder.line("object __entry;");
    builder.line("lock (_handles) { _handles.TryGetValue(id, out __entry); }");
    builder.line(&format!(
        "if (__entry is not {}InvokerDelegate fn)",
        wrapper
    ));
    builder.line("{");
    builder.indent();
    builder.line(&format!(
        "if (__entry != null) Console.Error.WriteLine($\"[azul] {} invoker: handle {{id}} is a \
         {{__entry.GetType().Name}}, not a {}InvokerDelegate\");",
        wrapper, wrapper
    ));
    builder.line("return;");
    builder.dedent();
    builder.line("}");
    builder.line("try");
    builder.line("{");
    builder.indent();
    builder.line(&format!("fn({});", args.join(", ")));
    builder.dedent();
    builder.line("}");
    builder.line("catch (Exception e)");
    builder.line("{");
    builder.indent();
    builder.line(&format!(
        "Console.Error.WriteLine($\"[azul] {} error: {{e.GetType().Name}}: \
         {{e.Message}}\\n{{e.StackTrace}}\");",
        wrapper
    ));
    builder.dedent();
    builder.line("}");
    builder.dedent();
    builder.line("};");
    builder.line(&format!("__Pin({});", var));
    builder.line(&format!(
        "NativeMethodsManaged.AzApp_set{}Invoker(System.Runtime.InteropServices.Marshal.\
         GetFunctionPointerForDelegate({}));",
        wrapper, var
    ));
    builder.blank();
}

/// `Register<X>(<X>InvokerDelegate raw)`: the escape hatch for callers
/// who want the raw pointers. Strongly typed on purpose — a delegate of
/// any other shape is a compile error, never a runtime surprise.
fn emit_register_raw(builder: &mut CodeBuilder, cb: &CallbackTypedefDef) {
    let wrapper = wrapper_name(cb);
    builder.line("/// <summary>");
    builder.line(&format!(
        "/// Register a raw {} invoker and wrap it in an Az{} struct a native call site can store.",
        wrapper, wrapper
    ));
    builder.line("/// </summary>");
    builder.line(&format!(
        "public static Az{w} Register{w}({w}InvokerDelegate raw)",
        w = wrapper
    ));
    builder.line("{");
    builder.indent();
    builder.line("if (raw == null) throw new ArgumentNullException(nameof(raw));");
    builder.line(&format!(
        "return NativeMethodsManaged.Az{}_createFromHostHandle(StoreHandle(raw));",
        wrapper
    ));
    builder.dedent();
    builder.line("}");
    builder.blank();
}

fn lower_first(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) => c.to_ascii_lowercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}

/// Emit `<X>WithData<T>` and `Register<X><T>(<X>WithData<T>)`. The
/// generic overload wraps the user's delegate in a raw invoker that
/// resolves the data slot (`RefanyGet as T`; a mismatch leaves `outPtr`
/// untouched so the engine keeps its default), reads each argument per
/// [`TypedArg`] and writes the result per [`TypedRet`].
fn emit_typed_delegate_and_register(
    builder: &mut CodeBuilder,
    cb: &CallbackTypedefDef,
    info: &TypedDelegateInfo,
) {
    let wrapper = &info.wrapper;

    // === Typed delegate ===
    builder.line("/// <summary>");
    builder.line(&format!(
        "/// Typed delegate for {}: `data` is the model passed at registration",
        wrapper
    ));
    builder.line("/// (no RefAny/IntPtr ceremony); the remaining arguments are the");
    builder.line("/// engine's values, wrapped in their wrapper classes where one exists.");
    builder.line("/// </summary>");
    let mut params = vec!["T data".to_string()];
    for (kind, name) in &info.args {
        let ty = match kind {
            TypedArg::Wrapper(t) => t.clone(),
            TypedArg::UnitEnum(e) => e.clone(),
            TypedArg::Bool => "bool".to_string(),
            TypedArg::Value(cs) => cs.clone(),
            TypedArg::RawIntPtr => "IntPtr".to_string(),
        };
        params.push(format!("{} {}", ty, name));
    }
    builder.line(&format!(
        "public delegate {} {}({}) where T : class;",
        info.ret.cs_type(),
        info.delegate_name(),
        params.join(", ")
    ));
    builder.blank();

    // === Register overload ===
    builder.line("/// <summary>");
    builder.line(&format!(
        "/// Register a typed {}. The data slot is resolved with `as T`;",
        info.delegate_name().replace('<', "&lt;").replace('>', "&gt;")
    ));
    builder.line("/// on a type mismatch the callback is skipped and the engine keeps its");
    builder.line("/// default return.");
    builder.line("/// </summary>");
    builder.line(&format!(
        "public static Az{w} Register{w}<T>({d} typed) where T : class",
        w = wrapper,
        d = info.delegate_name()
    ));
    builder.line("{");
    builder.indent();
    builder.line("if (typed == null) throw new ArgumentNullException(nameof(typed));");
    builder.line(&format!(
        "{}InvokerDelegate raw = ({}) =>",
        wrapper,
        invoker_params(cb).join(", ")
    ));
    builder.line("{");
    builder.indent();
    let data_slot = arg_name(&cb.args[0], 0);
    builder.line(&format!("var __data = RefanyGet({}) as T;", data_slot));
    builder.line("if (__data == null) return;");
    let mut call_args = vec!["__data".to_string()];
    for (kind, name) in &info.args {
        match kind {
            TypedArg::Wrapper(ty) => {
                builder.line(&format!(
                    "var __{n} = {ty}.__Borrow(System.Runtime.InteropServices.Marshal.\
                     PtrToStructure<{ffi}>({n}));",
                    n = name,
                    ty = ty,
                    ffi = ffi_type_name(ty)
                ));
                call_args.push(format!("__{}", name));
            }
            TypedArg::UnitEnum(e) => {
                builder.line(&format!(
                    "var __{n} = ({e}) System.Runtime.InteropServices.Marshal.ReadInt32({n});",
                    n = name,
                    e = e
                ));
                call_args.push(format!("__{}", name));
            }
            TypedArg::Bool => {
                builder.line(&format!(
                    "var __{n} = System.Runtime.InteropServices.Marshal.ReadByte({n}) != 0;",
                    n = name
                ));
                call_args.push(format!("__{}", name));
            }
            TypedArg::Value(cs) => {
                builder.line(&format!(
                    "var __{n} = System.Runtime.InteropServices.Marshal.PtrToStructure<{cs}>({n});",
                    n = name,
                    cs = cs
                ));
                call_args.push(format!("__{}", name));
            }
            TypedArg::RawIntPtr => call_args.push(name.clone()),
        }
    }
    let call = format!("typed({})", call_args.join(", "));
    match &info.ret {
        TypedRet::Void => builder.line(&format!("{};", call)),
        TypedRet::UnitEnum(_) => {
            builder.line(&format!("var __result = {};", call));
            builder
                .line("System.Runtime.InteropServices.Marshal.WriteInt32(outPtr, (int)__result);");
        }
        TypedRet::Bool => {
            builder.line(&format!("var __result = {};", call));
            builder.line(
                "System.Runtime.InteropServices.Marshal.WriteByte(outPtr, __result ? (byte)1 : \
                 (byte)0);",
            );
        }
        TypedRet::Value(_) => {
            builder.line(&format!("var __result = {};", call));
            builder.line(
                "System.Runtime.InteropServices.Marshal.StructureToPtr(__result, outPtr, false);",
            );
        }
        TypedRet::Wrapper(ty) => {
            builder.line(&format!("var __result = {};", call));
            builder.line("if (__result == null) return;");
            // The engine takes ownership of the bytes written through
            // outPtr; __Consume() neuters the wrapper so neither
            // Dispose() nor the finalizer frees them again.
            builder.line(&format!(
                "var __raw = ({}) __result.Raw;",
                ffi_type_name(ty)
            ));
            builder.line(
                "System.Runtime.InteropServices.Marshal.StructureToPtr(__raw, outPtr, false);",
            );
            builder.line("__result.__Consume();");
        }
    }
    builder.dedent();
    builder.line("};");
    builder.line(&format!("return Register{}(raw);", wrapper));
    builder.dedent();
    builder.line("}");
    builder.blank();
}
