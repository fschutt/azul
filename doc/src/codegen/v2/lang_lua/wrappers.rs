//! Idiomatic Lua wrapper layer.
//!
//! For each `Az<TypeName>` struct/enum that has a corresponding C-ABI
//! `_delete` (i.e. is a non-Copy heap-owning type) we emit:
//!
//! ```lua
//! local <TypeName>_methods = { ... }
//! ffi.metatype('Az<TypeName>', {
//!     __index = <TypeName>_methods,
//!     __gc    = function(self) C.az_<typename>_delete(self) end,
//! })
//! azul.<TypeName> = { create = function(...) return C.Az<TypeName>_create(...) end, ... }
//! ```
//!
//! For unit-only enums (every variant is `Unit`) we emit a flat constant
//! table:
//!
//! ```lua
//! azul.<EnumName> = { Variant1 = C.Az<EnumName>_Variant1, ... }
//! ```
//!
//! Wrapper types use the *unprefixed* name (`azul.App`, not `azul.AzApp`)
//! and instance methods drop the leading `<TypeName>_` so callers write
//! `app:run(window)` instead of `C.AzApp_run(app, window)`.
//!
//! # Skipped categories
//!
//! - `TypeCategory::Recursive`        — same reason as Python.
//! - `TypeCategory::VecRef`           — raw slice pointers, host-only.
//! - `TypeCategory::Boxed`            — internal heap wrappers.
//! - `TypeCategory::GenericTemplate`  — generic shells.
//! - `TypeCategory::DestructorOrClone`— internal callback typedefs.
//! - `TypeCategory::CallbackTypedef`  — function-pointer typedefs (the
//!   user-facing `CallbackDataPair` wrapper *is* emitted; consumers cast
//!   their Lua callbacks via `ffi.cast('Az<CallbackTypedefName>', fn)`).

use super::super::ir::{
    CodegenIR, EnumDef, EnumVariantKind, FunctionDef, FunctionKind, StructDef, TypeCategory,
};
use super::super::managed_lang_helpers::has_callback_arg;

/// Generate the full wrapper section as a single Lua source string.
///
/// The output begins and ends with a blank line so it inserts cleanly
/// between the cdef/load lines and the trailing `return azul`.
pub fn generate_wrappers(ir: &CodegenIR) -> String {
    let mut out = String::new();
    out.push('\n');

    // Unit-only enums become flat constant tables.
    out.push_str("-- ------------------------------------------------------------------\n");
    out.push_str("-- Unit-only enums (constant tables)\n");
    out.push_str("-- ------------------------------------------------------------------\n\n");
    for e in &ir.enums {
        if !should_emit_enum(e) {
            continue;
        }
        if !is_unit_only_enum(e) {
            continue;
        }
        emit_unit_enum(&mut out, e);
    }

    // Structs / data-bearing enums get a methods table + metatype.
    out.push_str("\n-- ------------------------------------------------------------------\n");
    out.push_str("-- Wrapper types (structs + tagged unions)\n");
    out.push_str("-- ------------------------------------------------------------------\n\n");

    for s in &ir.structs {
        if !should_emit_struct(s) {
            continue;
        }
        emit_struct_wrapper(&mut out, ir, s);
    }

    for e in &ir.enums {
        if !should_emit_enum(e) {
            continue;
        }
        if is_unit_only_enum(e) {
            continue; // already emitted above
        }
        emit_data_enum_wrapper(&mut out, ir, e);
    }

    out
}

// ============================================================================
// Filters
// ============================================================================

fn should_emit_struct(s: &StructDef) -> bool {
    if !s.generic_params.is_empty() {
        return false;
    }
    match s.category {
        TypeCategory::Recursive
        | TypeCategory::VecRef
        | TypeCategory::Boxed
        | TypeCategory::GenericTemplate
        | TypeCategory::DestructorOrClone
        | TypeCategory::CallbackTypedef => false,
        _ => true,
    }
}

fn should_emit_enum(e: &EnumDef) -> bool {
    if !e.generic_params.is_empty() {
        return false;
    }
    match e.category {
        TypeCategory::Recursive
        | TypeCategory::VecRef
        | TypeCategory::Boxed
        | TypeCategory::GenericTemplate
        | TypeCategory::DestructorOrClone
        | TypeCategory::CallbackTypedef => false,
        _ => true,
    }
}

fn is_unit_only_enum(e: &EnumDef) -> bool {
    !e.is_union
        && e.variants
            .iter()
            .all(|v| matches!(v.kind, EnumVariantKind::Unit))
}

// ============================================================================
// Unit-only enums
// ============================================================================

fn emit_unit_enum(out: &mut String, e: &EnumDef) {
    let lua_name = &e.name; // unprefixed in `azul.<Name>`
    let c_prefix = format!("Az{}", e.name);

    out.push_str(&format!("azul.{} = {{\n", lua_name));
    for v in &e.variants {
        // C-ABI emits unit enum members as `Az<Enum>_<Variant>`.
        out.push_str(&format!(
            "    {} = C.{}_{},\n",
            v.name, c_prefix, v.name
        ));
    }
    out.push_str("}\n\n");
}

// ============================================================================
// Struct wrappers
// ============================================================================

fn emit_struct_wrapper(out: &mut String, ir: &CodegenIR, s: &StructDef) {
    let class = &s.name; // e.g. "App"
    let c_name = format!("Az{}", s.name);

    let funcs: Vec<&FunctionDef> = ir.functions_for_class(class).collect();
    if funcs.is_empty() {
        return;
    }

    let has_delete = funcs.iter().any(|f| f.kind == FunctionKind::Delete);

    // The whole methods-table block is wrapped in `do ... end` so the
    // `<Class>_methods` local doesn't count against Lua's main-chunk limit
    // of 200 active locals. The metatype binding inside the block keeps
    // the table reachable via LuaJIT's internal metatype registry, so the
    // local is free to drop out of scope at the closing `end`.
    out.push_str("do\n");
    out.push_str(&format!("    local {}_methods = {{}}\n", class));

    // Instance methods (Method, MethodMut) — receiver `self`.
    let mut method_count = 0;
    for f in &funcs {
        match f.kind {
            FunctionKind::Method | FunctionKind::MethodMut => {
                emit_instance_method(out, class, &f.method_name, f);
                method_count += 1;
            }
            FunctionKind::DeepCopy => {
                // Expose deep-copy as `:clone()`.
                out.push_str(&format!(
                    "    function {}_methods:clone() return C.{}(self) end\n",
                    class, f.c_name
                ));
                method_count += 1;
            }
            FunctionKind::DebugToString => {
                out.push_str(&format!(
                    "    function {}_methods:toString() return C.{}(self) end\n",
                    class, f.c_name
                ));
                method_count += 1;
            }
            _ => {}
        }
    }
    if method_count == 0 {
        out.push_str(&format!("    -- (no instance methods on {})\n", class));
    }

    // AzString gets a `:to_lua_string()` method that decodes the
    // wrapped UTF-8 bytes into a Lua string. LuaJIT's `ffi.string`
    // copies `len` bytes from `ptr` — `self.vec.ptr` / `self.vec.len`
    // are accessible directly since AzString is a cdata with the C
    // struct layout.
    if class == "String" {
        out.push_str("    function String_methods:to_lua_string()\n");
        out.push_str("        if self.vec.ptr == nil or self.vec.len == 0 then return '' end\n");
        out.push_str("        return ffi.string(self.vec.ptr, self.vec.len)\n");
        out.push_str("    end\n");
    }

    // AzVec<T>:to_lua_array() — returns a Lua table with the elements
    // copied out. LuaJIT cdata supports indexing the typed pointer so
    // `self.ptr[i]` works for primitive elements directly; for struct
    // elements the user receives a cdata view (no copy).
    if s.category == TypeCategory::Vec {
        out.push_str(&format!(
            "    function {}_methods:to_lua_array()\n",
            class
        ));
        out.push_str("        if self.ptr == nil or self.len == 0 then return {} end\n");
        out.push_str("        local t = {}\n");
        out.push_str("        for i = 0, tonumber(self.len) - 1 do\n");
        out.push_str("            t[i + 1] = self.ptr[i]\n");
        out.push_str("        end\n");
        out.push_str("        return t\n");
        out.push_str("    end\n");
    }

    // Metatype binding — only for non-Copy types (those with _delete).
    // For Copy types we still want __index for instance methods, but no __gc.
    if has_delete {
        let delete_c = format!("Az{}_delete", class);
        out.push_str(&format!(
            "    ffi.metatype('{}', {{ __index = {}_methods, __gc = function(self) C.{}(self) end }})\n",
            c_name, class, delete_c
        ));
    } else if method_count > 0 {
        out.push_str(&format!(
            "    ffi.metatype('{}', {{ __index = {}_methods }})\n",
            c_name, class
        ));
    }
    out.push_str("end\n");

    // Module-level constructors / static methods (outside the do-block:
    // they hang off `azul`, no scoping concern).
    out.push_str(&format!("azul.{} = {{\n", class));
    for f in &funcs {
        match f.kind {
            FunctionKind::Constructor | FunctionKind::StaticMethod | FunctionKind::Default => {
                emit_static_method(out, &f.method_name, f);
            }
            _ => {}
        }
    }
    out.push_str("}\n\n");
}

// ============================================================================
// Tagged-union (data-bearing) enum wrappers
// ============================================================================

fn emit_data_enum_wrapper(out: &mut String, ir: &CodegenIR, e: &EnumDef) {
    let class = &e.name;
    let c_name = format!("Az{}", e.name);

    let funcs: Vec<&FunctionDef> = ir.functions_for_class(class).collect();
    let has_delete = funcs.iter().any(|f| f.kind == FunctionKind::Delete);

    // See struct equivalent for the rationale: scope the methods table so
    // we don't blow Lua's 200-locals-per-function ceiling on the main chunk.
    out.push_str("do\n");
    out.push_str(&format!("    local {}_methods = {{}}\n", class));

    let mut method_count = 0;
    for f in &funcs {
        match f.kind {
            FunctionKind::Method | FunctionKind::MethodMut => {
                emit_instance_method(out, class, &f.method_name, f);
                method_count += 1;
            }
            FunctionKind::DeepCopy => {
                out.push_str(&format!(
                    "    function {}_methods:clone() return C.{}(self) end\n",
                    class, f.c_name
                ));
                method_count += 1;
            }
            _ => {}
        }
    }
    // AzOption<T>:to_opt() / is_some / is_none — Lua nullable mirror.
    let mut auto_method_count = 0usize;
    if class.starts_with("Option") && e.variants.len() == 2 {
        let has_some_with_payload = e.variants.iter().any(|v| {
            v.name == "Some" && matches!(&v.kind, EnumVariantKind::Tuple(t) if t.len() == 1)
        });
        if has_some_with_payload {
            out.push_str(&format!(
                "    function {}_methods:to_opt()\n",
                class
            ));
            out.push_str("        if self.Some.tag == 0 then return nil end\n");
            out.push_str("        return self.Some.payload\n");
            out.push_str("    end\n");
            out.push_str(&format!(
                "    function {}_methods:is_some() return self.Some.tag ~= 0 end\n",
                class
            ));
            out.push_str(&format!(
                "    function {}_methods:is_none() return self.Some.tag == 0 end\n",
                class
            ));
            auto_method_count += 3;
        }
    }

    // AzResult<T,E>:unwrap() / is_ok / is_err — Lua mirror of the
    // Java/Kotlin/C#/Ruby helpers.
    if class.starts_with("Result") && e.variants.len() == 2 {
        let has_ok_with_payload = e.variants.iter().any(|v| {
            v.name == "Ok" && matches!(&v.kind, EnumVariantKind::Tuple(t) if t.len() == 1)
        });
        let has_err = e.variants.iter().any(|v| v.name == "Err");
        if has_ok_with_payload && has_err {
            out.push_str(&format!(
                "    function {}_methods:unwrap()\n",
                class
            ));
            out.push_str("        if self.Ok.tag == 0 then return self.Ok.payload end\n");
            out.push_str(&format!(
                "        error('{} unwrap on Err: ' .. tostring(self.Err.payload))\n",
                class
            ));
            out.push_str("    end\n");
            out.push_str(&format!(
                "    function {}_methods:is_ok() return self.Ok.tag == 0 end\n",
                class
            ));
            out.push_str(&format!(
                "    function {}_methods:is_err() return self.Ok.tag ~= 0 end\n",
                class
            ));
            auto_method_count += 3;
        }
    }

    if method_count == 0 && auto_method_count == 0 {
        out.push_str(&format!("    -- (no instance methods on {})\n", class));
    }

    if has_delete {
        let delete_c = format!("Az{}_delete", class);
        out.push_str(&format!(
            "    ffi.metatype('{}', {{ __index = {}_methods, __gc = function(self) C.{}(self) end }})\n",
            c_name, class, delete_c
        ));
    } else if method_count + auto_method_count > 0 {
        out.push_str(&format!(
            "    ffi.metatype('{}', {{ __index = {}_methods }})\n",
            c_name, class
        ));
    }
    out.push_str("end\n");

    // Module-level: variant constructors (one per non-Unit variant) + static
    // methods. Variant constructors come from FunctionKind::EnumVariantConstructor.
    out.push_str(&format!("azul.{} = {{\n", class));

    // Tags for unit-variant inspection (`if x.tag == azul.Foo.Tag.Bar then ...`).
    out.push_str("    Tag = {\n");
    for v in &e.variants {
        out.push_str(&format!(
            "        {} = C.{}_Tag_{},\n",
            v.name, c_name, v.name
        ));
    }
    out.push_str("    },\n");

    for f in &funcs {
        match f.kind {
            FunctionKind::EnumVariantConstructor
            | FunctionKind::Constructor
            | FunctionKind::StaticMethod
            | FunctionKind::Default => {
                emit_static_method(out, &f.method_name, f);
            }
            _ => {}
        }
    }
    out.push_str("}\n\n");
}

// ============================================================================
// Method-body emitters
// ============================================================================
//
// Two output forms:
//
// * Instance method (lives inside a `do ... end` block, base indent 4 spaces):
//       function Class_methods:method(...) ... end
// * Static method (lives inside `azul.Class = { ... }` literal, base indent 4):
//       method = function(...) ... end,
//
// Both branch on `has_callback_arg(func)`:
//
// * No callback args → keep the simple varargs forwarder, which forwards
//   all incoming args verbatim.
// * Has callback args → emit an explicit parameter list and inject a
//   `arg = azul.pin_callback('AzFooCallbackType', arg)` line for each
//   callback-typed arg before the C call.

/// Emit one instance method line (the do-block and `local Foo_methods = {}`
/// are emitted by the caller). `func.args[0]` is the receiver (named after
/// the class) and is supplied implicitly via `self`.
fn emit_instance_method(out: &mut String, class: &str, lua_method: &str, func: &FunctionDef) {
    let lua_method = sanitize_lua_ident(lua_method);

    if !has_callback_arg(func) {
        out.push_str(&format!(
            "    function {}_methods:{}(...) return C.{}(self, ...) end\n",
            class, lua_method, func.c_name
        ));
        return;
    }

    let visible: Vec<String> = func
        .args
        .iter()
        .skip(1)
        .map(|a| sanitize_lua_ident(&a.name))
        .collect();

    // Open: 4-space indent (inside the do-block).
    out.push_str(&format!(
        "    function {}_methods:{}({})\n",
        class,
        lua_method,
        visible.join(", ")
    ));

    // Body: 8-space indent (inside the function body, inside the do-block).
    emit_callback_pin_lines(out, "        ", &func.args[1..], &visible);

    let mut call_args = vec!["self".to_string()];
    call_args.extend(visible.iter().cloned());
    out.push_str(&format!(
        "        return C.{}({})\n",
        func.c_name,
        call_args.join(", ")
    ));
    out.push_str("    end\n");
}

/// Emit one entry of a static-method table:
///     method = function(args) ... end,
fn emit_static_method(out: &mut String, lua_method: &str, func: &FunctionDef) {
    let lua_method = sanitize_lua_ident(lua_method);

    if !has_callback_arg(func) {
        out.push_str(&format!(
            "    {} = function(...) return C.{}(...) end,\n",
            lua_method, func.c_name
        ));
        return;
    }

    let visible: Vec<String> = func
        .args
        .iter()
        .map(|a| sanitize_lua_ident(&a.name))
        .collect();

    // Special-case 1: a static constructor whose ONLY callback-typed arg's
    // wrapper name matches the function's return type — `Callback::create`,
    // `LayoutCallback::create` and friends. The C-ABI function takes a
    // raw function pointer (`AzCallbackType`) and re-wraps it via
    // `From<CallbackType>` with `ctx: None`, throwing away whatever
    // host-handle the host-invoker path baked in. Bypass it: the
    // `_register_callback` result already IS the wrapper struct we want
    // to return.
    let cb_args: Vec<&super::super::ir::FunctionArg> = func
        .args
        .iter()
        .filter(|a| a.callback_info.is_some())
        .collect();
    if cb_args.len() == 1 && func.args.iter().all(|a| {
        a.callback_info.is_some() || matches!(a.ref_kind, super::super::ir::ArgRefKind::Owned)
    }) {
        let cb = cb_args[0].callback_info.as_ref().unwrap();
        let wrapper_name = cb.callback_wrapper_name.as_str();
        let returns_self_wrapper = func
            .return_type
            .as_deref()
            .map(|t| t.trim() == wrapper_name)
            .unwrap_or(false);
        let supported = super::super::managed_host_invoker::HOST_INVOKER_KINDS
            .contains(&wrapper_name);

        if returns_self_wrapper && supported && func.args.len() == 1 {
            // Direct passthrough — the registered wrapper IS the return.
            let arg_name = sanitize_lua_ident(&func.args[0].name);
            out.push_str(&format!(
                "    {} = function({})\n",
                lua_method, arg_name
            ));
            out.push_str(&format!(
                "        return azul._register_callback('{}', {})\n",
                wrapper_name, arg_name
            ));
            out.push_str("    end,\n");
            return;
        }
    }

    // Special-case 2: `WindowCreateOptions::create(layout_callback)` —
    // the C-ABI takes a raw function pointer and calls
    // `LayoutCallback::create(cb)` internally, which discards any ctx.
    // We bypass that by constructing the options via `_default()` and
    // assigning the wrapper directly to `window_state.layout_callback`
    // — the same path the framework's own constructor uses, just with
    // the host-handle ctx preserved.
    if func.c_name == "AzWindowCreateOptions_create"
        && cb_args.len() == 1
        && cb_args[0]
            .callback_info
            .as_ref()
            .map(|c| c.callback_wrapper_name == "LayoutCallback")
            .unwrap_or(false)
    {
        let arg_name = sanitize_lua_ident(&func.args[0].name);
        out.push_str(&format!(
            "    {} = function({})\n",
            lua_method, arg_name
        ));
        out.push_str(&format!(
            "        local _cb = azul._register_callback('LayoutCallback', {})\n",
            arg_name
        ));
        out.push_str("        local _opts = C.AzWindowCreateOptions_default()\n");
        out.push_str("        _opts.window_state.layout_callback = _cb\n");
        out.push_str("        return _opts\n");
        out.push_str("    end,\n");
        return;
    }

    // Open: 4-space indent (inside the table literal).
    out.push_str(&format!(
        "    {} = function({})\n",
        lua_method,
        visible.join(", ")
    ));

    // Body: 8-space indent.
    emit_callback_pin_lines(out, "        ", &func.args[..], &visible);

    out.push_str(&format!(
        "        return C.{}({})\n",
        func.c_name,
        visible.join(", ")
    ));
    out.push_str("    end,\n");
}

/// Emit a callback-arg coercion line for every callback-typed entry in
/// `args`. Today we route through `azul._register_callback(<kind>, fn)`
/// which uses libazul's `_createFromHostHandle` constructor under the
/// hood — that produces an `AzCallback` / `AzLayoutCallback` *struct* by
/// value, not just a function pointer, so we substitute the variable in
/// place and the C-ABI function receives the wrapper struct.
///
/// The kind name comes from the wrapper struct (callback typedef "Foo"
/// belongs to wrapper "Foo" — IR pre-strips the trailing "Type"). We
/// only support the two kinds wired up in PR 1; unknown kinds fall back
/// to the legacy `pin_callback` so wrappers still emit valid code for
/// callback types that haven't received a `_createFromHostHandle` yet.
fn emit_callback_pin_lines(
    out: &mut String,
    indent: &str,
    args: &[super::super::ir::FunctionArg],
    names: &[String],
) {
    for (i, a) in args.iter().enumerate() {
        let Some(cb) = a.callback_info.as_ref() else {
            continue;
        };
        let wrapper_name = cb.callback_wrapper_name.as_str();
        let abi_takes_wrapper = !a.type_name.ends_with("Type");

        if super::super::managed_host_invoker::HOST_INVOKER_KINDS.contains(&wrapper_name) {
            // Host-invoker path. We hand the user-supplied Lua function to
            // `azul._register_callback`, which goes through libazul's
            // `_createFromHostHandle` constructor under the hood and
            // returns the matching `AzCallback` / `AzLayoutCallback`
            // wrapper struct.
            //
            // What we substitute for the variable depends on the C-ABI
            // signature api.json declared:
            //   * If the arg type is the *wrapper struct* (e.g. "Callback"),
            //     pass the whole struct — its `.ctx` carries our host handle.
            //   * If the arg type is the *raw function pointer typedef*
            //     (e.g. "CallbackType"), pass just `.cb`. The static thunk
            //     in libazul still routes through the host invoker, but
            //     ANY ctx is dropped at the C boundary because the C ABI
            //     doesn't carry it. Functions in this shape need a
            //     special-case fixup elsewhere (see emit_static_method's
            //     WindowCreateOptions::create branch).
            if abi_takes_wrapper {
                out.push_str(&format!(
                    "{indent}{n} = azul._register_callback('{w}', {n})\n",
                    indent = indent,
                    n = names[i],
                    w = wrapper_name
                ));
            } else {
                out.push_str(&format!(
                    "{indent}local _{n}_cb = azul._register_callback('{w}', {n})\n",
                    indent = indent,
                    n = names[i],
                    w = wrapper_name
                ));
                out.push_str(&format!(
                    "{indent}{n} = _{n}_cb.cb\n",
                    indent = indent,
                    n = names[i]
                ));
            }
        } else {
            // Legacy path for callback kinds without a host-invoker yet.
            // Keeps the wrapper output well-formed; binding still loads,
            // but the resulting cast won't execute on libffi-style hosts.
            let cb_typename = format!("Az{}", cb.callback_typedef_name);
            out.push_str(&format!(
                "{indent}{n} = azul.pin_callback('{ty}', {n})\n",
                indent = indent,
                n = names[i],
                ty = cb_typename
            ));
        }
    }
}

/// Sanitize an arg name for use as a Lua identifier. The IR uses
/// snake_case names from api.json, which already avoids most clashes;
/// we only need to suffix Lua reserved words (`end`, `local`, …) so the
/// generated `function f(local) ... end` doesn't fail to parse.
fn sanitize_lua_ident(name: &str) -> String {
    const LUA_RESERVED: &[&str] = &[
        "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "goto", "if",
        "in", "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
    ];
    if LUA_RESERVED.contains(&name) {
        format!("{}_", name)
    } else {
        name.to_string()
    }
}
