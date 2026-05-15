//! Shared host-invoker codegen helpers — data utilities used by every
//! managed-FFI language adapter (Lua, Ruby, PHP, Node-koffi, Common Lisp,
//! C#, Java, Kotlin, PowerShell, Perl, OCaml, …).
//!
//! ## What this module is
//!
//! The host-invoker pattern (see `azul_core::host_invoker`) lets managed
//! runtimes register a per-callback-kind closure with libazul; the static
//! thunk inside libazul then dispatches across the C-ABI on every fire.
//! Each language's `lang_<X>/managed.rs` is responsible for emitting the
//! per-language syntax — `ffi.cdef[[…]]` for LuaJIT, `attach_function`
//! for ruby-ffi, `[DllImport]` for C#, `cffi:defcfun` for Common Lisp,
//! and so on. The structure of the prelude is identical across languages,
//! but the surface syntax differs enough that a single text-template
//! engine would be more obfuscation than help.
//!
//! What *is* identical, and therefore lives here:
//!
//! * **The kind allowlist** — which callback wrappers actually have
//!   `impl_managed_callback!` applied on the Rust side. Adding a kind
//!   bumps one constant rather than one entry per language adapter.
//! * **IR filtering** — given the IR, return only the callback typedefs
//!   that map to a kind in the allowlist.
//! * **Type-name mapping** — Rust IR primitive → cdef C name (`u32` →
//!   `uint32_t`, `f64` → `double`, …). Every adapter needs this for the
//!   per-kind invoker signature.
//! * **Arg-name normalisation** — the IR sometimes carries empty arg
//!   names; emitters must fall back to a positional default.
//! * **Return-presence** — does the callback return a non-void value, i.e.
//!   does the host-side invoker need an out-pointer parameter?
//!
//! The Lua/Ruby adapters previously each carried their own copy of these
//! helpers; this module is the single source of truth they delegate to.

use super::ir::{CallbackTypedefDef, CodegenIR};

/// Wrapper names that have `impl_managed_callback!` applied on the Rust
/// side, and therefore export
/// `Az<Wrapper>_createFromHostHandle` + `AzApp_set<Wrapper>Invoker` from
/// libazul.
///
/// **Adding a kind:** apply `impl_managed_callback!` in
/// `azul-core` (or in the widget file that owns the wrapper), recompile
/// `libazul`, then append the wrapper name here. The codegen for every
/// managed-FFI adapter automatically picks it up via [`host_invoker_kinds`].
///
/// Entries here that aren't in `ir.callback_typedefs` are silently ignored
/// (this is the path that handles api.json renames). Entries in the IR
/// that aren't in this list get the legacy `pin_callback` path on the
/// wrapper-emitter side, which compiles fine but won't fire on
/// libffi-style hosts.
pub const HOST_INVOKER_KINDS: &[&str] = &[
    // Core framework callbacks (core/src/callbacks.rs, layout/src/callbacks.rs).
    "Callback",
    "LayoutCallback",
    "VirtualViewCallback",
    // Widget callbacks (layout/src/widgets/*).
    "ButtonOnClickCallback",
    "TabOnClickCallback",
    "CheckBoxOnToggleCallback",
    "TreeViewOnNodeClickCallback",
    "DropDownOnChoiceChangeCallback",
    "ColorInputOnValueChangeCallback",
    "FileInputOnPathChangeCallback",
    "NumberInputOnValueChangeCallback",
    "NumberInputOnFocusLostCallback",
    "TextInputOnTextInputCallback",
    "TextInputOnVirtualKeyDownCallback",
    "TextInputOnFocusLostCallback",
    "ListViewOnLazyLoadScrollCallback",
    "ListViewOnColumnClickCallback",
    "ListViewOnRowClickCallback",
    "RibbonOnTabClickCallback",
    // ThreadCallback fires on a worker thread (spawned by
    // Thread::create). Per-language host-invoker thunks for this kind
    // MUST acquire the host VM lock before dispatching — see
    // scripts/BINDING_STRATEGY_PER_LANGUAGE.md for the per-VM table
    // (PyGILState_Ensure, rb_thread_call_with_gvl, AttachCurrentThread,
    // etc.). Single-threaded interpreters (Lua, Perl, PHP, Pharo)
    // can't safely receive this callback; users should use the
    // writeback-only pattern (Rust extern "C" worker fn + host
    // WriteBackCallback on main).
    "ThreadCallback",
];

/// Filter `ir.callback_typedefs` down to the entries whose wrapper name is
/// in [`HOST_INVOKER_KINDS`].
pub fn host_invoker_kinds(ir: &CodegenIR) -> impl Iterator<Item = &CallbackTypedefDef> {
    ir.callback_typedefs.iter().filter(|cb| {
        let wrapper = wrapper_name(cb);
        HOST_INVOKER_KINDS.contains(&wrapper)
    })
}

/// `CallbackTypedefDef.name` is e.g. `"CallbackType"`. Strip the trailing
/// `"Type"` to recover the wrapper struct name (`"Callback"`), which is
/// the identifier used in C-ABI exports (`AzCallback_createFromHostHandle`,
/// `AzApp_setCallbackInvoker`) and in language-side dispatch tables.
pub fn wrapper_name(cb: &CallbackTypedefDef) -> &str {
    cb.name.strip_suffix("Type").unwrap_or(cb.name.as_str())
}

/// Phase J.2 detector: does this struct `s` have a layout-callback
/// constructor pattern, i.e. a `_default` static factory AND a
/// static factory whose only arg is a LayoutCallback fn-pointer
/// typedef AND returns the owning class? When matched, the binding
/// emits a smart `<class>.create(<host_sam>)` factory that registers
/// the SAM via the host invoker and splices the wrapper struct into
/// the WCO's nested `window_state.layout_callback` field — preserving
/// the host-handle ctx that the raw `_create(rawFnPtr)` path discards.
///
/// Today only `WindowCreateOptions` matches; the predicate is metadata-
/// driven so a future class with the same pattern lights up
/// automatically.
pub fn has_layout_callback_factory(
    s: &super::ir::StructDef,
    ir: &super::ir::CodegenIR,
) -> bool {
    layout_callback_factory_info(s, ir).is_some()
}

/// Metadata for emitting a smart `<class>.create(<host_sam>)` factory.
/// Everything the per-binding emitter needs is derived from the IR
/// scan — no class names, factory names, or field paths are hardcoded
/// in the language emitters.
#[derive(Clone, Debug)]
pub struct LayoutCallbackFactoryInfo {
    /// The owning class name (e.g. `"WindowCreateOptions"`). Same as
    /// `s.name` — surfaced here so the emitter doesn't repeat the
    /// derivation.
    pub class_name: String,
    /// The `_default` factory function's C-ABI name (e.g.
    /// `"AzWindowCreateOptions_default"`). The emitter calls this to
    /// build the empty struct that the callback bytes splice into.
    pub default_c_name: String,
    /// The callback kind wrapper (e.g. `"LayoutCallback"`). Used to
    /// pick the `Az<Wrapper>` ByValue struct + the host-invoker
    /// register method name.
    pub callback_wrapper: String,
    /// Dotted path from the owning class to the embedded callback
    /// field (e.g. `["window_state", "layout_callback"]`). The
    /// emitter joins this with the language's field-access syntax.
    pub field_path: Vec<String>,
}

/// Like [`has_layout_callback_factory`] but returns the IR-derived
/// metadata needed for per-binding emission. None when the struct
/// doesn't match the pattern.
pub fn layout_callback_factory_info(
    s: &super::ir::StructDef,
    ir: &super::ir::CodegenIR,
) -> Option<LayoutCallbackFactoryInfo> {
    use super::ir::FunctionKind;
    let default_func = ir.functions.iter().find(|f| {
        f.class_name == s.name && matches!(f.kind, FunctionKind::Default)
    })?;
    let create_func = ir.functions.iter().find(|f| {
        f.class_name == s.name
            && matches!(f.kind, FunctionKind::Constructor | FunctionKind::StaticMethod)
            && f.args.len() == 1
            && f.args[0]
                .callback_info
                .as_ref()
                .is_some()
            && f.return_type.as_deref().map(|r| r.trim() == s.name).unwrap_or(false)
    })?;
    let callback_wrapper = create_func.args[0]
        .callback_info
        .as_ref()?
        .callback_wrapper_name
        .clone();
    if !HOST_INVOKER_KINDS.contains(&callback_wrapper.as_str()) {
        return None;
    }
    // Find the field in `s` (recursively) whose type matches the
    // callback wrapper struct. The C ABI splices the registered
    // callback bytes into this field; the path tells the emitter
    // where to write.
    let field_path = find_field_of_type(s, &callback_wrapper, ir)?;
    Some(LayoutCallbackFactoryInfo {
        class_name: s.name.clone(),
        default_c_name: default_func.c_name.clone(),
        callback_wrapper,
        field_path,
    })
}

/// Recursively scan `s` for a field whose type is the named struct.
/// Returns the dotted path; None if not found. Used to discover where
/// the smart factory should splice the registered-callback bytes.
fn find_field_of_type(
    s: &super::ir::StructDef,
    target_type: &str,
    ir: &super::ir::CodegenIR,
) -> Option<Vec<String>> {
    for f in &s.fields {
        let tn = f.type_name.trim();
        if tn == target_type {
            return Some(vec![f.name.clone()]);
        }
        if let Some(child) = ir.find_struct(tn) {
            if let Some(sub) = find_field_of_type(child, target_type, ir) {
                let mut path = vec![f.name.clone()];
                path.extend(sub);
                return Some(path);
            }
        }
    }
    None
}

/// Phase J.1 detector: shared across every language binding. If `func`
/// is a `with_on_*(self, data: RefAny, callback: <Wrapper>)` method
/// whose `Wrapper` is in [`HOST_INVOKER_KINDS`], return `Some((smart,
/// wrapper))` where `smart` is the snake-case sibling method name
/// (`"on_click"` etc.) and `wrapper` is the callback wrapper kind
/// (`"Callback"`, `"ButtonOnClickCallback"`, ...).
///
/// Requires args[2].type_name to match the wrapper struct name (NOT
/// the fn-pointer typedef form). When the API exposes the typedef
/// form (`CheckBoxOnToggleCallbackType` etc.) the smart factory would
/// need an extra `.cb` field extract bridge; today only Button matches
/// the wrapper-struct shape.
pub fn smart_callback_setter_info(
    func: &super::ir::FunctionDef,
) -> Option<(String, String)> {
    use super::ir::FunctionKind;
    if !matches!(
        func.kind,
        FunctionKind::Method | FunctionKind::MethodMut | FunctionKind::DeepCopy
    ) {
        return None;
    }
    if func.args.len() != 3 {
        return None;
    }
    if func.args[1].type_name != "RefAny" {
        return None;
    }
    let cb_info = func.args[2].callback_info.as_ref()?;
    if !HOST_INVOKER_KINDS.contains(&cb_info.callback_wrapper_name.as_str()) {
        return None;
    }
    if func.args[2].type_name != cb_info.callback_wrapper_name {
        return None;
    }
    let smart = func.method_name.strip_prefix("with_")?;
    Some((smart.to_string(), cb_info.callback_wrapper_name.clone()))
}

/// Map a Rust/IR type name to its cdef C-ABI type name.
///
/// Primitives lower to their `<stdint.h>` / `<stdbool.h>` form; non-primitives
/// get the `Az` prefix. The result is what an FFI cdef block (LuaJIT,
/// PHP FFI, koffi, CFFI, …) wants to see for the per-kind invoker
/// pointer-arg signature.
pub fn c_typename(rust_type: &str) -> String {
    match rust_type {
        "u8" => "uint8_t".to_string(),
        "u16" => "uint16_t".to_string(),
        "u32" => "uint32_t".to_string(),
        "u64" => "uint64_t".to_string(),
        "i8" => "int8_t".to_string(),
        "i16" => "int16_t".to_string(),
        "i32" => "int32_t".to_string(),
        "i64" => "int64_t".to_string(),
        "f32" => "float".to_string(),
        "f64" => "double".to_string(),
        "usize" => "size_t".to_string(),
        "isize" => "ssize_t".to_string(),
        "bool" => "bool".to_string(),
        "()" | "void" => "void".to_string(),
        _ => format!("Az{}", rust_type),
    }
}

/// True when the callback typedef returns a non-void value, i.e. the
/// host-side invoker needs an out-pointer for the return.
pub fn has_return(cb: &CallbackTypedefDef) -> bool {
    cb.return_type
        .as_deref()
        .map(|s| s != "void")
        .unwrap_or(false)
}

/// The cdef C-ABI return type name (e.g. `"AzUpdate"`, `"AzStyledDom"`).
/// Returns `None` when the callback returns void.
pub fn return_c_typename(cb: &CallbackTypedefDef) -> Option<String> {
    let rt = cb.return_type.as_deref()?;
    if rt == "void" {
        None
    } else {
        Some(c_typename(rt))
    }
}

/// A stable name for the i-th positional arg of a callback. Falls back to
/// `_arg{i}` when the IR didn't carry one (rare; some legacy entries had
/// empty names).
pub fn arg_name_or_default(cb: &CallbackTypedefDef, idx: usize) -> String {
    cb.args
        .get(idx)
        .filter(|a| !a.name.is_empty())
        .map(|a| a.name.clone())
        .unwrap_or_else(|| format!("_arg{}", idx))
}

/// Render the C-ABI argument list for one callback kind's invoker, as a
/// comma-separated string suitable for splatting into a `typedef void
/// (*Az<K>Invoker)(<args>);` declaration.
///
/// Layout: `uint64_t` (host handle id), one `const <T>*` per arg (pointer
/// args are libffi-friendly; the static thunk on the Rust side does the
/// by-value plumbing), then a trailing `<R>*` out-param when the callback
/// returns non-void.
pub fn invoker_c_arg_list(cb: &CallbackTypedefDef) -> String {
    let mut parts = vec!["uint64_t".to_string()];
    for arg in &cb.args {
        parts.push(format!("const {}*", c_typename(&arg.type_name)));
    }
    if let Some(ret) = return_c_typename(cb) {
        parts.push(format!("{}*", ret));
    }
    parts.join(", ")
}

/// Emit the host-invoker cdef block for languages with C-syntax FFI
/// parsers (LuaJIT `ffi.cdef`, PHP `FFI::cdef`, koffi `decl`, CFFI free-form
/// declarations). Output is plain C with `/* … */` comments; no
/// preprocessor directives, no `extern "C"`, parseable by every libffi-
/// adjacent host.
///
/// Languages with declarative FFI (ruby-ffi `attach_function`, JNA
/// interface methods, P/Invoke `[DllImport]`) translate these by hand.
///
/// The block is self-contained — it declares `void`-returning functions
/// against pre-existing types (`AzRefAny`, `AzCallback`, `AzCallbackInfo`,
/// …). It must be emitted *after* whatever host-language cdef block
/// declares those types, otherwise the FFI parser will reject the
/// forward references.
pub fn emit_cdef_block(out: &mut String, ir: &CodegenIR) {
    out.push_str(
        "    /* Host-handle releaser — called once per RefAny last-clone drop. */\n",
    );
    out.push_str("    void AzApp_setHostHandleReleaser(void (*)(uint64_t));\n\n");
    out.push_str(
        "    /* User-data RefAny on top of the host-handle path: one shared\n",
    );
    out.push_str(
        "       lifetime story for both callback registration and refany_create. */\n",
    );
    out.push_str("    AzRefAny AzRefAny_newHostHandle(uint64_t);\n");
    out.push_str("    uint64_t AzRefAny_getHostHandle(const AzRefAny*);\n\n");
    out.push_str(
        "    /* Generic invoker fallback — fires when no per-kind invoker is\n",
    );
    out.push_str(
        "       registered. Useful for hosts that want one dispatch site for\n",
    );
    out.push_str(
        "       every kind, or for user-defined custom kinds shipped via a\n",
    );
    out.push_str(
        "       small downstream `impl_managed_callback!` whose host hasn't\n",
    );
    out.push_str(
        "       wired a per-kind setter. Args are an array of pointers (one\n",
    );
    out.push_str(
        "       per by-value frame arg, in declared order). */\n",
    );
    out.push_str("    void AzApp_setGenericInvoker(\n");
    out.push_str(
        "        void (*)(uint64_t, const char*, const void* const*, size_t, void*));\n\n",
    );
    out.push_str(
        "    /* Per-kind invoker setters + pointer-arg signatures. The return\n",
    );
    out.push_str(
        "       value is an *out-parameter* so libffi-style runtimes (which\n",
    );
    out.push_str(
        "       can't return aggregates > 8 bytes from callbacks) handle every\n",
    );
    out.push_str("       kind uniformly. */\n");
    for cb in host_invoker_kinds(ir) {
        let wrapper = wrapper_name(cb);
        let arg_list = invoker_c_arg_list(cb);
        out.push_str(&format!(
            "    typedef void (*Az{w}Invoker)({args});\n",
            w = wrapper,
            args = arg_list
        ));
        out.push_str(&format!(
            "    void AzApp_set{w}Invoker(Az{w}Invoker);\n",
            w = wrapper
        ));
        out.push_str(&format!(
            "    Az{w} Az{w}_createFromHostHandle(uint64_t);\n",
            w = wrapper
        ));
    }
}

/// Convenience: each callback arg's `(name, c_type_with_pointer)` pair,
/// suitable for splatting into a function-signature emitter. Pointer
/// args: every callback arg in the host invoker is passed by pointer
/// (this is the libffi-friendly convention; the static thunk on the
/// Rust side does the by-value plumbing).
pub fn invoker_arg_pairs(cb: &CallbackTypedefDef) -> Vec<(String, String)> {
    cb.args
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let nm = if a.name.is_empty() {
                format!("_arg{}", i)
            } else {
                a.name.clone()
            };
            let ty = format!("const {}*", c_typename(&a.type_name));
            (nm, ty)
        })
        .collect()
}

/// CamelCase → snake_case (e.g. `"VirtualViewCallback"` →
/// `"virtual_view_callback"`). Adapters whose host language uses
/// snake_case identifiers (Ruby, Python, CFFI hyphen-case) call this on
/// wrapper names to derive consistent symbol names.
pub fn to_snake_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i != 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// CamelCase → kebab-case (e.g. `"VirtualViewCallback"` →
/// `"virtual-view-callback"`). Used by Common Lisp / Scheme adapters.
pub fn to_kebab_case(name: &str) -> String {
    to_snake_case(name).replace('_', "-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snake_case_handles_camel_and_acronyms() {
        assert_eq!(to_snake_case("Callback"), "callback");
        assert_eq!(to_snake_case("LayoutCallback"), "layout_callback");
        assert_eq!(to_snake_case("VirtualViewCallback"), "virtual_view_callback");
    }

    #[test]
    fn kebab_case_replaces_underscores() {
        assert_eq!(to_kebab_case("LayoutCallback"), "layout-callback");
    }

    #[test]
    fn c_typename_maps_primitives() {
        assert_eq!(c_typename("u32"), "uint32_t");
        assert_eq!(c_typename("usize"), "size_t");
        assert_eq!(c_typename("f64"), "double");
        assert_eq!(c_typename("bool"), "bool");
        assert_eq!(c_typename("()"), "void");
        assert_eq!(c_typename("void"), "void");
    }

    #[test]
    fn c_typename_prefixes_user_types() {
        assert_eq!(c_typename("Update"), "AzUpdate");
        assert_eq!(c_typename("RefAny"), "AzRefAny");
    }
}
