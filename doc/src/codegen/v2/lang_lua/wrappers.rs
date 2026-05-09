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
    if method_count == 0 {
        out.push_str(&format!("    -- (no instance methods on {})\n", class));
    }

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
        if wrapper_name == "Callback" || wrapper_name == "LayoutCallback" {
            // Host-invoker path: the C-ABI function for the *wrapper struct*
            // is what setOnClick / WindowCreateOptions::create take, not the
            // raw function pointer. We hand the user-supplied Lua function
            // to `_register_callback` and pass the resulting struct.
            //
            // Note: a few API entry points (e.g. `WindowCreateOptions::create`)
            // are typed against the bare callback typedef, not the wrapper
            // — for those we extract the `.cb` field. The C-ABI function
            // pointer in `.cb` is a static thunk inside libazul that knows
            // to dispatch through the registered host invoker.
            out.push_str(&format!(
                "{indent}local _{n}_cb = azul._register_callback('{w}', {n})\n",
                indent = indent,
                n = names[i],
                w = wrapper_name
            ));
            // Some C entry points take the wrapper struct by value
            // (e.g. `Dom_addCallback` takes `AzCallback`); others take the
            // bare typedef (`AzCallbackType` / `AzLayoutCallbackType`,
            // e.g. `WindowCreateOptions_create`). We always feed the
            // struct's `.cb` slot, which is the static thunk pointer —
            // identical between the two API shapes.
            out.push_str(&format!(
                "{indent}{n} = _{n}_cb.cb\n",
                indent = indent,
                n = names[i]
            ));
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
