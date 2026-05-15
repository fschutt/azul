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

    // Phase J.1 (Lua): same shared detector as the other bindings.
    // Emit `:<smart>(data, fn)` for every method matching
    // with_on_*(self, RefAny, <CallbackWrapperStruct>).
    for func in ir.functions_for_class(&s.name) {
        let Some((smart_snake, _wrapper_kind)) =
            super::super::managed_host_invoker::smart_callback_setter_info(func)
        else {
            continue;
        };
        out.push_str(&format!(
            "    function {}_methods:{}(data, fn)\n",
            class, smart_snake
        ));
        out.push_str(
            "        local data_ref = azul.refany_create(data)\n",
        );
        out.push_str(&format!(
            "        return self:{}(data_ref, fn)\n",
            func.method_name
        ));
        out.push_str("    end\n");
    }

    // CC-4 (Lua): fluent `:with(opts)` builder. Recursively assigns
    // nested table fields into the underlying cdata struct, auto-
    // converting Lua strings to AzString. Returns self for chain
    // composition with `:with_*` builder methods. Pure cdata-driven;
    // no per-field allow-list. Routes through `azul._apply_opts` for
    // the recursion logic (defined in the module postlude).
    //
    // Drops user-visible drilling like
    //   window.window_state.title = azul._az_string('...')
    // in favor of
    //   window:with({ window_state = { title = 'Hello World' } })
    out.push_str(&format!(
        "    function {}_methods:with(opts) azul._apply_opts(self, opts); return self end\n",
        class
    ));

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
    // copied out. For primitive `self.ptr` (`*uint8`, `*int32`, …),
    // `self.ptr[i]` is a value read — safe past the Vec being closed.
    // For struct `self.ptr` (`*AzDom`, …), `self.ptr[i]` is a cdata
    // overlay onto the Vec's buffer — would dangle if the Vec is
    // closed. Clone each element via `Az<T>_clone` (when available)
    // so the yielded entries own independent heap allocations.
    if s.category == TypeCategory::Vec {
        out.push_str(&format!(
            "    function {}_methods:to_lua_array()\n",
            class
        ));
        out.push_str("        if self.ptr == nil or self.len == 0 then return {} end\n");
        out.push_str("        local t = {}\n");
        out.push_str("        for i = 0, tonumber(self.len) - 1 do\n");

        // Detect the element type from the first field. The IR
        // stores it as `*const T` / `*mut T` or sometimes bare `T`
        // depending on the source; strip the pointer-kind prefix
        // when present, otherwise pass the bare name through (same
        // logic as Java's `detect_vec_elem_type_jvm` at
        // `lang_java/wrappers.rs:455`).
        let elem_rust: Option<String> = s.fields.first().map(|f| {
            let raw = f.type_name.trim();
            raw.strip_prefix("*const ")
                .or_else(|| raw.strip_prefix("*mut "))
                .map(|t| t.trim().to_string())
                .unwrap_or_else(|| raw.to_string())
        });
        let is_primitive = elem_rust
            .as_deref()
            .map(|t| {
                matches!(
                    t,
                    "u8" | "i8"
                        | "u16"
                        | "i16"
                        | "u32"
                        | "i32"
                        | "u64"
                        | "i64"
                        | "f32"
                        | "f64"
                        | "bool"
                        | "usize"
                        | "isize"
                )
            })
            .unwrap_or(false);
        let has_clone = elem_rust
            .as_deref()
            .map(|t| {
                ir.functions
                    .iter()
                    .any(|f| f.class_name == t && matches!(f.kind, FunctionKind::DeepCopy))
            })
            .unwrap_or(false);

        if is_primitive {
            out.push_str("            t[i + 1] = self.ptr[i]\n");
        } else if has_clone {
            let elem_ty = elem_rust.as_deref().unwrap_or("");
            out.push_str(&format!(
                "            t[i + 1] = C.Az{}_clone(self.ptr[i])\n",
                elem_ty
            ));
        } else {
            out.push_str("            -- WARNING: element has no _clone — borrowed view dangles if the Vec is closed.\n");
            out.push_str("            t[i + 1] = self.ptr[i]\n");
        }

        out.push_str("        end\n");
        out.push_str("        return t\n");
        out.push_str("    end\n");
    }

    // Phase I.2.7 (Lua): __eq metamethod via Az<X>_partialEq when
    // TypeTraits.is_partial_eq and the helper is exported.
    let eq_sym = format!("Az{}_partialEq", s.name);
    let has_eq = s.traits.is_partial_eq
        && ir.functions.iter().any(|f| f.c_name == eq_sym);
    let eq_clause = if has_eq {
        format!(", __eq = function(a, b) return C.{}(a, b) end", eq_sym)
    } else {
        String::new()
    };

    // Phase I.3.5 (Lua): __tostring metamethod via Az<X>_toDbgString.
    let dbg_sym = format!("Az{}_toDbgString", s.name);
    let has_dbg = s.traits.is_debug
        && ir.functions.iter().any(|f| f.c_name == dbg_sym)
        && s.name != "String";
    let tostring_clause = if has_dbg {
        format!(
            ", __tostring = function(self) \
             local az = C.{}(self); \
             if az.vec.ptr == nil or az.vec.len == 0 then return '' end; \
             return ffi.string(az.vec.ptr, tonumber(az.vec.len)) \
             end",
            dbg_sym
        )
    } else {
        String::new()
    };

    // Phase I.1.8 (Lua): __len + __index for numeric keys when this is
    // a Vec wrapper. Enables `#vec` length, `vec[i]` access (0-based at
    // the C ABI, 1-based via convention here), and `ipairs(vec)`-like
    // iteration via the standard length+index protocol.
    let is_vec = s.fields.len() == 4
        && s.fields[0].name == "ptr"
        && s.fields[1].name == "len"
        && s.fields[2].name == "cap"
        && s.fields[1].type_name.trim() == "usize";
    let len_clause = if is_vec {
        ", __len = function(self) return tonumber(self.len) end".to_string()
    } else {
        String::new()
    };

    // Metatype binding — only for non-Copy types (those with _delete).
    // For Copy types we still want __index for instance methods, but no __gc.
    if has_delete {
        let delete_c = format!("Az{}_delete", class);
        out.push_str(&format!(
            "    ffi.metatype('{}', {{ __index = {}_methods, __gc = function(self) C.{}(self) end{}{}{} }})\n",
            c_name, class, delete_c, eq_clause, tostring_clause, len_clause
        ));
    } else if method_count > 0 || has_eq || has_dbg || is_vec {
        out.push_str(&format!(
            "    ffi.metatype('{}', {{ __index = {}_methods{}{}{} }})\n",
            c_name, class, eq_clause, tostring_clause, len_clause
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
    // AzOption<T>:to_opt() / is_some / is_none — Lua nullable mirror
    // with delete+clone semantics (mirrors Ruby/JVM commits 75a1fbcd2
    // + memory_safety_session_2026_05_15).
    let mut auto_method_count = 0usize;
    if e.variants.len() == 2 {
        let some_payload = e
            .variants
            .iter()
            .find(|v| v.name == "Some")
            .and_then(|v| match &v.kind {
                EnumVariantKind::Tuple(t) if t.len() == 1 => Some(&t[0].0),
                _ => None,
            });
        let has_none = e.variants.iter().any(|v| v.name == "None");
        if let (Some(payload_ty), true) = (some_payload, has_none) {
            emit_lua_to_opt_body(out, class, payload_ty, ir);
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
    if e.variants.len() == 2 {
        let ok_payload = e
            .variants
            .iter()
            .find(|v| v.name == "Ok")
            .and_then(|v| match &v.kind {
                EnumVariantKind::Tuple(t) if t.len() == 1 => Some(&t[0].0),
                _ => None,
            });
        let has_err = e.variants.iter().any(|v| v.name == "Err");
        if let (Some(payload_ty), true) = (ok_payload, has_err) {
            emit_lua_unwrap_body(out, class, payload_ty, ir);
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

    // Detect Owned `String` args. When present, we have to enumerate
    // args (can't use the `(...)` varargs passthrough) so we can route
    // each one through `azul._az_string(...)`. Mirrors the auto-string
    // rule in Java/Kotlin/C#/Ruby/Node.
    let has_az_string = func.args.iter().any(is_az_string_owned_arg);

    // Consume-after-by-value (mirrors lang_java/kotlin/csharp's
    // `consume_after_call` walk landed in 62094b885). Any arg whose IR
    // ref_kind is Owned has its bytes transferred to Rust by the C
    // call; LuaJIT's __gc metatype handler would otherwise re-run
    // Az<X>_delete on those now-Rust-owned bytes. `azul._consume`
    // (defined in lang_lua/managed.rs) calls `ffi.gc(c, nil)` to
    // detach the finalizer per-instance. Safe on primitives — the
    // helper type-checks for cdata first.
    let consumed_self = func
        .args
        .first()
        .map(|a| matches!(a.ref_kind, super::super::ir::ArgRefKind::Owned))
        .unwrap_or(false);
    let consumed_arg_indices: Vec<usize> = func
        .args
        .iter()
        .enumerate()
        .skip(1)
        .filter(|(_, a)| matches!(a.ref_kind, super::super::ir::ArgRefKind::Owned))
        .map(|(i, _)| i)
        .collect();

    // Phase I.5.5 (Lua): Option/Result auto-unwrap at the wrapper
    // boundary. Routes through the per-cdata `:to_opt()` / `:unwrap()`
    // methods emitted by A.1.4 via ffi.metatype.
    let unwrap_call = match func.return_type.as_deref().map(str::trim) {
        Some(rt) if rt.starts_with("Option") => Some(":to_opt()"),
        Some(rt) if rt.starts_with("Result") => Some(":unwrap()"),
        _ => None,
    };

    let needs_consume = consumed_self || !consumed_arg_indices.is_empty();

    if !has_callback_arg(func) && !has_az_string && unwrap_call.is_none() && !needs_consume {
        out.push_str(&format!(
            "    function {}_methods:{}(...) return C.{}(self, ...) end\n",
            class, lua_method, func.c_name
        ));
        return;
    }

    if !has_callback_arg(func) && !has_az_string && !needs_consume {
        // Auto-unwrap only path: keep the varargs varadic, wrap the return.
        let unwrap = unwrap_call.unwrap();
        out.push_str(&format!(
            "    function {}_methods:{}(...) return (C.{}(self, ...)){} end\n",
            class, lua_method, func.c_name, unwrap
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

    // Body: 8-space indent.
    emit_callback_pin_lines(out, "        ", &func.args[1..], &visible);

    let mut call_args = vec!["self".to_string()];
    for (i, a) in func.args.iter().skip(1).enumerate() {
        if is_az_string_owned_arg(a) {
            call_args.push(format!("azul._az_string({})", visible[i]));
        } else {
            call_args.push(visible[i].clone());
        }
    }
    // Phase I.5.5 (Lua): auto-unwrap Option/Result return at the body
    // end. Reused detection from above (varargs short-circuit path).
    let unwrap_call = match func.return_type.as_deref().map(str::trim) {
        Some(rt) if rt.starts_with("Option") => Some(":to_opt()"),
        Some(rt) if rt.starts_with("Result") => Some(":unwrap()"),
        _ => None,
    };
    // Capture the result before emitting consume calls (statements
    // can't follow a `return`), then return at the end.
    let consume_lines: Vec<String> = {
        let mut v = Vec::new();
        for idx in &consumed_arg_indices {
            v.push(format!("        azul._consume({})", visible[*idx - 1]));
        }
        if consumed_self {
            v.push("        azul._consume(self)".to_string());
        }
        v
    };

    if consume_lines.is_empty() {
        match unwrap_call {
            Some(uw) => out.push_str(&format!(
                "        return (C.{}({})){}\n",
                func.c_name,
                call_args.join(", "),
                uw
            )),
            None => out.push_str(&format!(
                "        return C.{}({})\n",
                func.c_name,
                call_args.join(", ")
            )),
        }
    } else {
        // Multi-line: capture, consume, return.
        match unwrap_call {
            Some(uw) => out.push_str(&format!(
                "        local _ret = (C.{}({})){}\n",
                func.c_name,
                call_args.join(", "),
                uw
            )),
            None => out.push_str(&format!(
                "        local _ret = C.{}({})\n",
                func.c_name,
                call_args.join(", ")
            )),
        }
        for line in &consume_lines {
            out.push_str(line);
            out.push('\n');
        }
        out.push_str("        return _ret\n");
    }
    out.push_str("    end\n");
}

/// Auto-string-conversion rule (mirrors Java/Kotlin/C#/Ruby/Node):
/// any Owned `String` arg accepts a plain Lua string at the wrapper
/// level. The call site routes the value through `azul._az_string`
/// (defined in `mod.rs` postlude). Pure type-driven; no method-name
/// allowlist.
/// True iff the IR exports `Az<payload_ty>_clone` (FunctionKind::DeepCopy).
fn lua_has_clone(payload_ty: &str, ir: &CodegenIR) -> bool {
    ir.functions
        .iter()
        .any(|f| f.class_name == payload_ty && matches!(f.kind, FunctionKind::DeepCopy))
}

/// True iff the IR exports `Az<option_ty>_delete`.
fn lua_has_delete(option_ty: &str, ir: &CodegenIR) -> bool {
    ir.functions
        .iter()
        .any(|f| f.class_name == option_ty && matches!(f.kind, FunctionKind::Delete))
}

/// True iff the IR's struct for `payload_ty` is `TypeCategory::String`.
fn lua_payload_is_string(payload_ty: &str, ir: &CodegenIR) -> bool {
    use super::super::ir::TypeCategory;
    ir.find_struct(payload_ty)
        .map(|s| matches!(s.category, TypeCategory::String))
        .unwrap_or(false)
}

/// Emit Lua `:to_opt()` for an Az<Option> enum. Three extraction
/// shapes (mirrors Ruby and JVM/CLR): AzString decode / wrapper-class
/// clone-then-delete / primitive pass-through.
fn emit_lua_to_opt_body(
    out: &mut String,
    class: &str,
    payload_ty: &str,
    ir: &CodegenIR,
) {
    let option_name = format!("Az{}", class);
    let has_delete = lua_has_delete(class, ir);
    // Lua __gc fires later on the same cdata; we must call
    // `azul._consume(self)` after the explicit _delete to disarm
    // the metatype's finalizer, otherwise the cdata double-frees.
    let delete_line = if has_delete {
        format!(
            "        C.{}_delete(self)\n        azul._consume(self)\n",
            option_name
        )
    } else {
        String::new()
    };

    out.push_str(&format!("    function {}_methods:to_opt()\n", class));
    out.push_str("        if self.Some.tag == 0 then\n");
    if has_delete {
        // Inner-block indent: 12 spaces. delete_line carries 8;
        // prefix the first occurrence with 4 extra and re-indent
        // subsequent lines via str::replace.
        let inner = delete_line.replace("\n        ", "\n            ");
        out.push_str(&format!("    {}", inner));
    }
    out.push_str("            return nil\n");
    out.push_str("        end\n");

    if lua_payload_is_string(payload_ty, ir) {
        // AzString payload — decode bytes into a Lua string, then
        // delete the Option to free the Vec.ptr buffer.
        out.push_str("        local __azs = self.Some.payload\n");
        out.push_str("        local __out\n");
        out.push_str("        if __azs.vec.ptr == nil or __azs.vec.len == 0 then\n");
        out.push_str("            __out = \"\"\n");
        out.push_str("        else\n");
        out.push_str(
            "            __out = ffi.string(__azs.vec.ptr, tonumber(__azs.vec.len))\n",
        );
        out.push_str("        end\n");
        if has_delete {
            out.push_str(&delete_line);
        }
        out.push_str("        return __out\n");
    } else if lua_has_clone(payload_ty, ir) {
        // Wrapper / cloneable payload — clone for an independent
        // allocation, then delete the Option (drops the original
        // payload's heap allocations).
        out.push_str(&format!(
            "        local __cloned = C.Az{}_clone(self.Some.payload)\n",
            payload_ty
        ));
        if has_delete {
            out.push_str(&delete_line);
        }
        out.push_str("        return __cloned\n");
    } else {
        // Primitive / non-cloneable: capture the value before delete
        // so the local owns it independently.
        out.push_str("        local __val = self.Some.payload\n");
        if has_delete {
            out.push_str(&delete_line);
        }
        out.push_str("        return __val\n");
    }

    out.push_str("    end\n");
}

/// Emit Lua `:unwrap()` for an Az<Result> enum. Same three extraction
/// shapes as [`emit_lua_to_opt_body`]; Err branch raises before delete.
fn emit_lua_unwrap_body(
    out: &mut String,
    class: &str,
    payload_ty: &str,
    ir: &CodegenIR,
) {
    let result_name = format!("Az{}", class);
    let has_delete = lua_has_delete(class, ir);
    let delete_line = if has_delete {
        format!("        C.{}_delete(self)\n", result_name)
    } else {
        String::new()
    };

    out.push_str(&format!("    function {}_methods:unwrap()\n", class));
    out.push_str("        if self.Ok.tag ~= 0 then\n");
    out.push_str(&format!(
        "            error('{} unwrap on Err: ' .. tostring(self.Err.payload))\n",
        class
    ));
    out.push_str("        end\n");

    if lua_payload_is_string(payload_ty, ir) {
        out.push_str("        local __azs = self.Ok.payload\n");
        out.push_str("        local __out\n");
        out.push_str("        if __azs.vec.ptr == nil or __azs.vec.len == 0 then\n");
        out.push_str("            __out = \"\"\n");
        out.push_str("        else\n");
        out.push_str(
            "            __out = ffi.string(__azs.vec.ptr, tonumber(__azs.vec.len))\n",
        );
        out.push_str("        end\n");
        if has_delete {
            out.push_str(&delete_line);
        }
        out.push_str("        return __out\n");
    } else if lua_has_clone(payload_ty, ir) {
        out.push_str(&format!(
            "        local __cloned = C.Az{}_clone(self.Ok.payload)\n",
            payload_ty
        ));
        if has_delete {
            out.push_str(&delete_line);
        }
        out.push_str("        return __cloned\n");
    } else {
        out.push_str("        local __val = self.Ok.payload\n");
        if has_delete {
            out.push_str(&delete_line);
        }
        out.push_str("        return __val\n");
    }

    out.push_str("    end\n");
}

fn is_az_string_owned_arg(a: &super::super::ir::FunctionArg) -> bool {
    a.type_name.trim() == "String"
        && matches!(a.ref_kind, super::super::ir::ArgRefKind::Owned)
}

/// Emit one entry of a static-method table:
///     method = function(args) ... end,
fn emit_static_method(out: &mut String, lua_method: &str, func: &FunctionDef) {
    let lua_method = sanitize_lua_ident(lua_method);

    // When the func has Owned `String` args, switch from the varargs
    // passthrough to an enumerated form so we can route each through
    // `azul._az_string`.
    let has_az_string = func.args.iter().any(is_az_string_owned_arg);

    if !has_callback_arg(func) && !has_az_string {
        out.push_str(&format!(
            "    {} = function(...) return C.{}(...) end,\n",
            lua_method, func.c_name
        ));
        return;
    }

    if !has_callback_arg(func) && has_az_string {
        // Enumerated form for auto-string-conversion only.
        let visible: Vec<String> = func
            .args
            .iter()
            .map(|a| sanitize_lua_ident(&a.name))
            .collect();
        let mut call_args: Vec<String> = Vec::new();
        for (i, a) in func.args.iter().enumerate() {
            if is_az_string_owned_arg(a) {
                call_args.push(format!("azul._az_string({})", visible[i]));
            } else {
                call_args.push(visible[i].clone());
            }
        }
        out.push_str(&format!(
            "    {} = function({}) return C.{}({}) end,\n",
            lua_method,
            visible.join(", "),
            func.c_name,
            call_args.join(", "),
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
    // Phase J.4 (Lua): detect via type-driven rule rather than the
    // hardcoded `func.c_name == "AzWindowCreateOptions_create"` match.
    // Trigger: static factory whose only arg is a LayoutCallback fn-
    // pointer typedef AND whose return type would carry a nested
    // `window_state.layout_callback` field (which is true today only for
    // WindowCreateOptions; the body assumes that path).
    //
    // `LayoutCallback::create(cb)` internally would discard the host-
    // invoker ctx; the smart body bypasses by constructing via
    // `_default()` and splicing the wrapper struct into the embedded
    // layout_callback field, preserving ctx.
    let is_layout_constructor = func.args.len() == 1
        && cb_args.len() == 1
        && cb_args[0]
            .callback_info
            .as_ref()
            .map(|c| c.callback_wrapper_name == "LayoutCallback")
            .unwrap_or(false)
        && func.return_type.as_deref().map(|r| r.trim() == func.class_name).unwrap_or(false);
    if is_layout_constructor {
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

    // Route Owned `String` args through azul._az_string in line. The
    // helper is a pass-through for non-string values, so it's safe to
    // apply uniformly per-arg.
    let final_args: Vec<String> = func
        .args
        .iter()
        .enumerate()
        .map(|(i, a)| {
            if is_az_string_owned_arg(a) {
                format!("azul._az_string({})", visible[i])
            } else {
                visible[i].clone()
            }
        })
        .collect();
    out.push_str(&format!(
        "        return C.{}({})\n",
        func.c_name,
        final_args.join(", ")
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
