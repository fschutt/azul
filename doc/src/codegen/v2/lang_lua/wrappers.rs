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

    // Methods table.
    out.push_str(&format!("local {}_methods = {{}}\n", class));

    // Instance methods (Method, MethodMut) — receiver `self`.
    let mut method_count = 0;
    for f in &funcs {
        match f.kind {
            FunctionKind::Method | FunctionKind::MethodMut => {
                let lua_name = &f.method_name;
                out.push_str(&format!(
                    "function {}_methods:{}(...) return C.{}(self, ...) end\n",
                    class, lua_name, f.c_name
                ));
                method_count += 1;
            }
            FunctionKind::DeepCopy => {
                // Expose deep-copy as `:clone()`.
                out.push_str(&format!(
                    "function {}_methods:clone() return C.{}(self) end\n",
                    class, f.c_name
                ));
                method_count += 1;
            }
            FunctionKind::DebugToString => {
                out.push_str(&format!(
                    "function {}_methods:toString() return C.{}(self) end\n",
                    class, f.c_name
                ));
                method_count += 1;
            }
            _ => {}
        }
    }
    if method_count == 0 {
        out.push_str(&format!("-- (no instance methods on {})\n", class));
    }

    // Metatype binding — only for non-Copy types (those with _delete).
    // For Copy types we still want __index for instance methods, but no __gc.
    if has_delete {
        let delete_c = format!("Az{}_delete", class);
        out.push_str(&format!(
            "ffi.metatype('{}', {{ __index = {}_methods, __gc = function(self) C.{}(self) end }})\n",
            c_name, class, delete_c
        ));
    } else if method_count > 0 {
        out.push_str(&format!(
            "ffi.metatype('{}', {{ __index = {}_methods }})\n",
            c_name, class
        ));
    }

    // Module-level constructors / static methods.
    out.push_str(&format!("azul.{} = {{\n", class));
    for f in &funcs {
        match f.kind {
            FunctionKind::Constructor | FunctionKind::StaticMethod | FunctionKind::Default => {
                let lua_name = &f.method_name;
                out.push_str(&format!(
                    "    {} = function(...) return C.{}(...) end,\n",
                    lua_name, f.c_name
                ));
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

    out.push_str(&format!("local {}_methods = {{}}\n", class));

    let mut method_count = 0;
    for f in &funcs {
        match f.kind {
            FunctionKind::Method | FunctionKind::MethodMut => {
                out.push_str(&format!(
                    "function {}_methods:{}(...) return C.{}(self, ...) end\n",
                    class, f.method_name, f.c_name
                ));
                method_count += 1;
            }
            FunctionKind::DeepCopy => {
                out.push_str(&format!(
                    "function {}_methods:clone() return C.{}(self) end\n",
                    class, f.c_name
                ));
                method_count += 1;
            }
            _ => {}
        }
    }
    if method_count == 0 {
        out.push_str(&format!("-- (no instance methods on {})\n", class));
    }

    if has_delete {
        let delete_c = format!("Az{}_delete", class);
        out.push_str(&format!(
            "ffi.metatype('{}', {{ __index = {}_methods, __gc = function(self) C.{}(self) end }})\n",
            c_name, class, delete_c
        ));
    } else if method_count > 0 {
        out.push_str(&format!(
            "ffi.metatype('{}', {{ __index = {}_methods }})\n",
            c_name, class
        ));
    }

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
                out.push_str(&format!(
                    "    {} = function(...) return C.{}(...) end,\n",
                    f.method_name, f.c_name
                ));
            }
            _ => {}
        }
    }
    out.push_str("}\n\n");
}
