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
//! Emitted Lua must stay dual-runtime (LuaJIT `ffi` / cffi-lua): no
//! `type(x) == 'cdata'` (use `ffi.istype` or the `_is_cdata` shim), no
//! `ULL` literals — see `lang_lua/mod.rs`.
//!
//! Monomorphized generic aliases (`ClipPathValue = CssPropertyValue<ClipPath>`,
//! `PhysicalSizeU32 = PhysicalSize<u32>`) get the same treatment: they are
//! real `union` / `struct` types on the wire with the full derive export set,
//! they just live in `ir.type_aliases` instead of `ir.structs` / `ir.enums`
//! (see [`emit_alias_wrapper`]).
//!
//! # Skipped categories
//!
//! - `TypeCategory::VecRef`           — raw slice pointers, host-only.
//! - `TypeCategory::GenericTemplate`  — generic shells.
//! - `TypeCategory::DestructorOrClone`— internal callback typedefs.
//! - `TypeCategory::CallbackTypedef`  — function-pointer typedefs (the user-facing
//!   `CallbackDataPair` wrapper *is* emitted; consumers cast their Lua callbacks via
//!   `ffi.cast('Az<CallbackTypedefName>', fn)`).
//!
//! `Boxed` and `Recursive` are NOT skipped here — see
//! [`should_emit_struct`]: an opaque heap handle (`ImageRef`, `FontRef`,
//! `Texture`, `Svg`, `GlContextPtr`) and a self-referential type (`Xml`,
//! `XmlNode`) are both ordinary structs to an FFI that uses the header's
//! own layout, and a Lua program that cannot reach them cannot load an
//! image, draw with GL or parse XML.

use super::super::{
    ir::{
        ArgRefKind, CodegenIR, EnumDef, EnumVariantKind, FunctionArg, FunctionDef, FunctionKind,
        MonomorphizedKind, StructDef, TypeAliasDef, TypeCategory,
    },
    managed_lang_helpers::{has_callback_arg, is_refany_type},
};

/// Generate the full wrapper section as a single Lua source string.
///
/// The output begins and ends with a blank line so it inserts cleanly
/// between the cdef/load lines and the trailing `return azul`.
pub fn generate_wrappers(ir: &CodegenIR) -> String {
    let mut out = String::new();
    out.push('\n');

    // The derive surface every wrapper type shares, emitted once (see
    // DERIVE_HELPER) and called from each type's metatype line.
    out.push_str(DERIVE_HELPER);

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

    // Monomorphized generic aliases: neither `ir.structs` nor `ir.enums`
    // knows them, yet the C ABI has them as real types with a full set of
    // derive exports (see `emit_alias_wrapper`).
    out.push_str("\n-- ------------------------------------------------------------------\n");
    out.push_str("-- Monomorphized generic aliases (CssPropertyValue<T>, BoxOrStatic<T>, …)\n");
    out.push_str("-- ------------------------------------------------------------------\n\n");

    for a in &ir.type_aliases {
        emit_alias_wrapper(&mut out, ir, a);
    }

    // api.json's constants, merged onto the class tables the wrappers built.
    emit_constants(&mut out, ir);

    out
}

// ============================================================================
// Filters
// ============================================================================

/// Two categories other bindings skip are emitted here, because the reason
/// they are skipped is about generating a native wrapper STRUCT, which an
/// FFI binding never does:
///
/// * `Boxed` — an opaque heap handle (`ImageRef`, `FontRef`, `Texture`,
///   `Svg`, `GlContextPtr`) is an ordinary struct with a pointer field at
///   the C ABI; skipping it lost every one of their methods.
/// * `Recursive` — `Xml` / `XmlNode` / `XmlNodeChildVec` are "infinite
///   size" only for a binding that embeds the type by value in a wrapper
///   of its own; the cdef declares exactly what `azul.h` declares (the
///   recursion goes through the child Vec's `ptr`), so the FFI handles
///   them like any other type.
fn should_emit_struct(s: &StructDef) -> bool {
    if !s.generic_params.is_empty() {
        return false;
    }
    !matches!(s.category, TypeCategory::VecRef
        | TypeCategory::GenericTemplate
        | TypeCategory::DestructorOrClone
        | TypeCategory::CallbackTypedef)
}

fn should_emit_enum(e: &EnumDef) -> bool {
    if !e.generic_params.is_empty() {
        return false;
    }
    !matches!(e.category, TypeCategory::VecRef
        | TypeCategory::GenericTemplate
        | TypeCategory::DestructorOrClone
        | TypeCategory::CallbackTypedef)
}

fn is_unit_only_enum(e: &EnumDef) -> bool {
    !e.is_union
        && e.variants
            .iter()
            .all(|v| matches!(v.kind, EnumVariantKind::Unit))
}

// ============================================================================
// Derive surface (equality, ordering, hashing, debug, ownership)
// ============================================================================

/// The derive exports a class has, as the `syms` table `_az_derive` reads:
/// `(<key>, <C symbol>)` pairs in the order the helper documents them.
///
/// Built from the IR's FUNCTION list, not from `TypeTraits`: the export is
/// the only evidence a derive exists (a type whose api.json `derive` list
/// and whose `custom_impls` disagree still gets the symbol emitted into
/// libazul, and the binding must reach whatever libazul exports).
fn derive_syms(funcs: &[&FunctionDef]) -> Vec<(&'static str, String)> {
    const KEYS: &[(FunctionKind, &str)] = &[
        (FunctionKind::Delete, "delete"),
        (FunctionKind::DeepCopy, "clone"),
        (FunctionKind::PartialEq, "eq"),
        (FunctionKind::Cmp, "cmp"),
        (FunctionKind::PartialCmp, "pcmp"),
        (FunctionKind::Hash, "hash"),
        (FunctionKind::DebugToString, "dbg"),
    ];
    KEYS.iter()
        .filter_map(|(kind, key)| {
            funcs
                .iter()
                .find(|f| f.kind == *kind)
                .map(|f| (*key, f.c_name.clone()))
        })
        .collect()
}

/// Emit a type's `ffi.metatype` line: the methods table becomes `__index`
/// and [`DERIVE_HELPER`]'s `_az_derive` adds one metamethod per derive the
/// type exports.
///
/// `is_vec` asks for `#value` (the ptr/len/cap layout), `is_string` for a
/// `__tostring` that decodes the wrapped UTF-8 bytes instead of printing
/// the Debug form — the string type is the one type whose text IS its
/// value.
fn emit_metatype(
    out: &mut String,
    class: &str,
    c_name: &str,
    syms: &[(&'static str, String)],
    is_vec: bool,
    is_string: bool,
) {
    let mut fields: Vec<String> = syms
        .iter()
        .map(|(key, sym)| format!("{key} = '{sym}'"))
        .collect();
    if is_vec {
        fields.push("len = true".to_string());
    }
    if is_string {
        fields.push("str = true".to_string());
    }
    out.push_str(&format!(
        "    ffi.metatype('{c_name}', _az_derive({class}_methods, '{c_name}', {{ {} }}))\n",
        fields.join(", ")
    ));
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
        out.push_str(&format!("    {} = C.{}_{},\n", v.name, c_prefix, v.name));
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

    // Everything a `derive` gives the type (`_clone`, `_partialEq`,
    // `_cmp`, `_partialCmp`, `_hash`, `_toDbgString`, `_delete`) is
    // installed by `_az_derive` on the metatype line at the end of the
    // block, so the loop below only walks the api.json methods.
    let syms = derive_syms(&funcs);

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
        if matches!(f.kind, FunctionKind::Method | FunctionKind::MethodMut) {
            emit_instance_method(out, class, &f.method_name, f, ir);
            method_count += 1;
        }
    }
    if method_count == 0 && syms.is_empty() {
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
        out.push_str(&format!(
            "        return self:{}(data, fn)\n",
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
    let is_string = s.category == TypeCategory::String;
    if is_string {
        out.push_str(&format!("    function {}_methods:to_lua_string()\n", class));
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
        out.push_str(&format!("    function {}_methods:to_lua_array()\n", class));
        out.push_str("        if self.ptr == nil or self.len == 0 then return {} end\n");
        out.push_str("        local t = {}\n");
        out.push_str("        for i = 0, _tonum(self.len) - 1 do\n");

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
            // Each clone is an owned C-returned value: arm its
            // finalizer so dropping the Lua table entry frees it.
            let elem_ty = elem_rust.as_deref().unwrap_or("");
            out.push_str(&format!(
                "            t[i + 1] = {}\n",
                lua_arm(
                    &format!("C.Az{}_clone(self.ptr[i])", elem_ty),
                    &lua_finalizer_for(elem_ty, ir)
                )
            ));
        } else {
            out.push_str(
                "            -- WARNING: element has no _clone — borrowed view dangles if the Vec \
                 is closed.\n",
            );
            out.push_str("            t[i + 1] = self.ptr[i]\n");
        }

        out.push_str("        end\n");
        out.push_str("        return t\n");
        out.push_str("    end\n");
    }

    // Phase I.1.8 (Lua): `#vec` when this is a Vec wrapper. Decided by
    // the ptr/len/cap layout, never by the name.
    let is_vec = s.fields.len() == 4
        && s.fields[0].name == "ptr"
        && s.fields[1].name == "len"
        && s.fields[2].name == "cap"
        && s.fields[1].type_name.trim() == "usize";

    // Metatype binding: `__index` plus every metamethod the type's derive
    // exports support (`__gc`, `__eq`, `__lt`, `__le`, `__tostring`,
    // `__len`), all built by `_az_derive`. A type with neither methods nor
    // derives has nothing to attach.
    if method_count > 0 || !syms.is_empty() || is_vec {
        emit_metatype(out, class, &c_name, &syms, is_vec, is_string);
    }
    out.push_str("end\n");

    // Module-level constructors / static methods (outside the do-block:
    // they hang off `azul`, no scoping concern).
    out.push_str(&format!("azul.{} = {{\n", class));
    for f in &funcs {
        match f.kind {
            FunctionKind::Constructor | FunctionKind::StaticMethod | FunctionKind::Default => {
                emit_static_method(out, &f.method_name, f, ir);
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
    // Same derive surface as a struct — a tagged union compares, orders,
    // hashes and prints through the very same C exports.
    let syms = derive_syms(&funcs);

    // See struct equivalent for the rationale: scope the methods table so
    // we don't blow Lua's 200-locals-per-function ceiling on the main chunk.
    out.push_str("do\n");
    out.push_str(&format!("    local {}_methods = {{}}\n", class));

    let mut method_count = 0;
    for f in &funcs {
        if matches!(f.kind, FunctionKind::Method | FunctionKind::MethodMut) {
            emit_instance_method(out, class, &f.method_name, f, ir);
            method_count += 1;
        }
    }
    // AzOption<T>:to_opt() / is_some / is_none — Lua nullable mirror
    // with delete+clone semantics (mirrors Ruby/JVM commits 75a1fbcd2
    // + memory_safety_session_2026_05_15).
    let mut auto_method_count = 0usize;
    if e.variants.len() == 2 {
        let some_payload =
            e.variants
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

    if method_count == 0 && auto_method_count == 0 && syms.is_empty() {
        out.push_str(&format!("    -- (no instance methods on {})\n", class));
    }

    if method_count + auto_method_count > 0 || !syms.is_empty() {
        emit_metatype(out, class, &c_name, &syms, false, false);
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
                emit_static_method(out, &f.method_name, f, ir);
            }
            _ => {}
        }
    }
    out.push_str("}\n\n");
}

// ============================================================================
// Monomorphized generic aliases
// ============================================================================

/// Emit the wrapper for one monomorphized generic alias
/// (`ClipPathValue = CssPropertyValue<ClipPath>`, `BoxOrStaticString =
/// BoxOrStatic<String>`, `PhysicalSizeU32 = PhysicalSize<u32>`).
///
/// `ir.find_struct` / `ir.find_enum` do not know these names — they live in
/// `ir.type_aliases` and carry their instantiated shape in
/// `monomorphized_def` — but the C ABI has them as real
/// `union AzClipPathValue` / `struct AzPhysicalSizeU32` types, and
/// `functions_for_class` lists the derive exports the IR synthesises for
/// them. Skipping them skipped 119 classes' worth of equality, ordering and
/// hashing, plus the destructor of the 32 that own heap memory.
fn emit_alias_wrapper(out: &mut String, ir: &CodegenIR, a: &TypeAliasDef) {
    // A plain alias (`GLuint = u32`) IS its target — no type of its own.
    let Some(mono) = a.monomorphized_def.as_ref() else {
        return;
    };
    // A monomorphized SimpleEnum is a C `enum`: an int on the wire with no
    // struct/union ctype to attach a metatype to (`is_value_aggregate`
    // classifies it the same way); its variants are plain constants.
    let variants = match &mono.kind {
        MonomorphizedKind::TaggedUnion { variants, .. } => Some(variants),
        MonomorphizedKind::Struct { .. } => None,
        MonomorphizedKind::SimpleEnum { .. } => return,
    };

    let class = &a.name;
    let c_name = format!("Az{}", a.name);
    let funcs: Vec<&FunctionDef> = ir.functions_for_class(class).collect();
    let syms = derive_syms(&funcs);
    if syms.is_empty() {
        return;
    }

    out.push_str("do\n");
    out.push_str(&format!("    local {}_methods = {{}}\n", class));
    // An alias carries no api.json methods, only the derives — so the
    // struct shapes get the same fluent `:with(opts)` builder every other
    // struct wrapper has, and the tagged unions get their derives alone.
    if variants.is_none() {
        out.push_str(&format!(
            "    function {}_methods:with(opts) azul._apply_opts(self, opts); return self end\n",
            class
        ));
    }
    emit_metatype(out, class, &c_name, &syms, false, false);
    out.push_str("end\n");

    out.push_str(&format!("azul.{} = {{\n", class));
    if let Some(variants) = variants {
        // Tags for variant inspection (`if v.tag == azul.ClipPathValue.Tag.Exact`),
        // exactly like a data-bearing enum's.
        out.push_str("    Tag = {\n");
        for v in variants {
            out.push_str(&format!(
                "        {} = C.{}_Tag_{},\n",
                v.name, c_name, v.name
            ));
        }
        out.push_str("    },\n");
    }
    for f in &funcs {
        if f.kind == FunctionKind::Default {
            emit_static_method(out, &f.method_name, f, ir);
        }
    }
    out.push_str("}\n\n");
}

// ============================================================================
// Constants
// ============================================================================

/// Emit every `ir.constants` entry onto the namespace table of the class it
/// belongs to (`azul.GlContextPtr.COLOR_BUFFER_BIT`), so the OpenGL enum
/// values api.json carries are usable as
/// `gl:clear(azul.GlContextPtr.COLOR_BUFFER_BIT)` instead of being
/// unreachable from Lua.
///
/// The values are MERGED into whatever the wrapper layer already put on
/// `azul.<Class>` (its constructors and static methods) instead of
/// replacing it, and the table is created when the class has no wrapper at
/// all, so the constants never depend on emission order.
fn emit_constants(out: &mut String, ir: &CodegenIR) {
    if ir.constants.is_empty() {
        return;
    }
    // Group by class, first-seen order inside and out, so the output is
    // byte-stable across runs. `<Class>_<CONSTANT>` is the name
    // `ir_builder::build_constants` composes.
    let mut groups: Vec<(&str, Vec<(String, &str)>)> = Vec::new();
    for c in &ir.constants {
        let Some((class, _)) = c.name.split_once('_') else {
            continue;
        };
        let name = c.member_name();
        match groups.iter_mut().find(|(g, _)| *g == class) {
            Some((_, items)) => items.push((name, c.value.as_str())),
            None => groups.push((class, vec![(name, c.value.as_str())])),
        }
    }

    out.push_str("\n-- ------------------------------------------------------------------\n");
    out.push_str("-- Constants\n");
    out.push_str("-- ------------------------------------------------------------------\n\n");

    for (class, items) in groups {
        out.push_str(&format!(
            "-- api.json's constants for {class}, merged onto the class table.\n"
        ));
        out.push_str("do\n");
        out.push_str("    local consts = {\n");
        for (name, value) in items {
            // Uppercase constant names are valid Lua identifiers (every Lua
            // keyword is lowercase), but a name that is not takes the
            // bracket form rather than being renamed.
            let key = if is_lua_ident(&name) {
                name.to_string()
            } else {
                format!("[\"{name}\"]")
            };
            out.push_str(&format!("        {key} = {},\n", lua_constant_literal(value)));
        }
        out.push_str("    }\n");
        out.push_str(&format!("    azul.{class} = azul.{class} or {{}}\n"));
        out.push_str(&format!(
            "    for k, v in pairs(consts) do azul.{class}[k] = v end\n"
        ));
        out.push_str("end\n\n");
    }
}

/// Is `name` usable as a bare Lua table key? (Lua's keywords are all
/// lowercase, so an uppercase constant name never collides with one.)
fn is_lua_ident(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && sanitize_lua_ident(name) == name
}

/// The Lua expression for a constant's api.json value.
///
/// Lua's number is a double, so an integer above 2^53 would silently lose
/// bits as a literal (`GL_TIMEOUT_IGNORED` = `0xFFFFFFFFFFFFFFFF` becomes
/// 2^64 and reaches C as 0). Such a value is assembled into a 64-bit cdata
/// from its two halves instead — spelled without a `ULL` suffix, which is
/// LuaJIT-only (see the dual-runtime rules in `lang_lua/mod.rs`).
fn lua_constant_literal(value: &str) -> String {
    let text = value.trim();
    let parsed = match text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        Some(hex) => u64::from_str_radix(hex, 16).ok(),
        None => text.parse::<u64>().ok(),
    };
    match parsed {
        // Exactly representable as a Lua number: emit it as written, so the
        // hex spelling of a GL enum survives into the binding.
        Some(v) if v <= (1u64 << 53) => text.to_string(),
        Some(v) => format!(
            "(ffi.cast('uint64_t', 0x{:X}) * 4294967296 + 0x{:X})",
            v >> 32,
            v & 0xFFFF_FFFF
        ),
        // Not an integer we can reason about (a float, a negative, an
        // expression): pass the api.json text through unchanged.
        None => text.to_string(),
    }
}

// ============================================================================
// Method-body emitters
// ============================================================================
//
// Two output forms:
//
// * Instance method (lives inside a `do ... end` block, base indent 4 spaces): function
//   Class_methods:method(...) ... end
// * Static method (lives inside `azul.Class = { ... }` literal, base indent 4): method =
//   function(...) ... end,
//
// Both branch on `has_callback_arg(func)`:
//
// * No callback args → keep the simple varargs forwarder, which forwards all incoming args
//   verbatim.
// * Has callback args → emit an explicit parameter list and inject an
//   `azul._register_callback(<kind>, arg)` (host-invoker kinds) or
//   `azul.pin_callback('AzFooCallbackType', arg)` line for each callback-typed arg before the C
//   call.

/// Does `func` take a callback that libazul invokes on a WORKER thread?
///
/// Lua is a single-threaded VM with no runtime lock: a Lua function called
/// from a libazul worker thread corrupts the interpreter, so a setter for
/// one emits a hard error instead of a registration that would crash at
/// runtime. The writeback pattern is the supported route from Lua — the
/// WriteBackCallback runs on the main thread.
///
/// This is the one decision in this emitter that an api.json property
/// cannot make: nothing records which thread invokes a callback, and the
/// shape does not tell (plenty of main-thread callbacks take neither a
/// `CallbackInfo` nor a channel endpoint, so any structural rule would
/// refuse callbacks that work today). The kinds are therefore listed, and
/// the list is the exception that the marker below makes reviewable.
// allow-api-name: api.json records no "invoked off the main thread" property
const OFF_MAIN_THREAD_CALLBACKS: &[&str] = &["ThreadCallback"];

fn takes_off_main_thread_callback(func: &FunctionDef) -> bool {
    func.args.iter().any(|a| {
        a.callback_info
            .as_ref()
            .is_some_and(|c| OFF_MAIN_THREAD_CALLBACKS.contains(&c.callback_wrapper_name.as_str()))
    })
}

/// Emit one instance method line (the do-block and `local Foo_methods = {}`
/// are emitted by the caller). `func.args[0]` is the receiver (named after
/// the class) and is supplied implicitly via `self`.
fn emit_instance_method(
    out: &mut String,
    class: &str,
    lua_method: &str,
    func: &FunctionDef,
    ir: &CodegenIR,
) {
    let lua_method = sanitize_lua_ident(lua_method);

    // Worker-thread callback guard (see `takes_off_main_thread_callback`):
    // LuaJIT has no runtime lock, so a Lua fn invoked from a libazul
    // worker thread corrupts the VM. Applies to instance methods like
    // ThreadPool:create_thread.
    if takes_off_main_thread_callback(func) {
        out.push_str(&format!(
            "    function {}_methods:{}(...)\n\x20       error('ThreadCallback from Lua is \
             unsupported; use the writeback pattern', 2)\n\x20   end\n",
            class, lua_method
        ));
        return;
    }

    // Detect Owned `String` args. When present, we have to enumerate
    // args (can't use the `(...)` varargs passthrough) so we can route
    // each one through `azul._az_string(...)`. Mirrors the auto-string
    // rule in Java/Kotlin/C#/Ruby/Node.
    let has_az_string = func.args.iter().any(|a| is_az_string_owned_arg(a, ir));
    let has_refany = func.args.iter().any(|a| is_owned_refany_arg(a, ir));

    // Consume-after-by-value (mirrors lang_java/kotlin/csharp's
    // `consume_after_call` walk landed in 62094b885). Any owned by-value
    // arg of a deletable type has its bytes transferred to Rust by the C
    // call; the __gc metatype handler would otherwise re-run Az<X>_delete
    // on those now-Rust-owned bytes. `azul._consume` (lang_lua/managed.rs)
    // calls `ffi.gc(c, nil)` to detach the finalizer per instance. RefAny
    // args are exempt: `azul._refany_arg` hands the C call an unarmed
    // transient (see `emit_arg_coercions`).
    let consumed_self = func
        .args
        .first()
        .map(|a| matches!(a.ref_kind, ArgRefKind::Owned))
        .unwrap_or(false);
    let consumed_arg_indices: Vec<usize> = lua_consumable_arg_indices(func, ir)
        .into_iter()
        .filter(|i| *i > 0)
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

    // Ownership arming (see module docs): LuaJIT only arms metatype
    // __gc for ffi.new-created cdata — structs RETURNED BY VALUE from
    // C calls are collected WITHOUT running any finalizer. So every
    // owned return must be armed explicitly via
    // `ffi.gc(v, C.Az<T>_delete)` at the wrapper boundary. Auto-
    // unwrapped Option/Result temps are exempt: `:to_opt()`/`:unwrap()`
    // delete the shell themselves.
    let ret_fin = if unwrap_call.is_some() {
        None
    } else {
        func.return_type
            .as_deref()
            .and_then(|t| lua_finalizer_for(t, ir))
    };

    // CC-6 (Lua): void-returning mutator methods that DON'T consume self
    // (e.g. `add_child`, `set_button_type`, `add_css_property`) currently
    // emit `return C.AzFoo_addX(self, ...)`, which yields nil at the Lua
    // level — chaining `body:add_child(label):add_child(button)` errors
    // with `attempt to index a nil value`. Re-emit them as
    // `C.AzFoo_addX(self, ...); return self` so the receiver flows
    // through the chain. We deliberately exclude `consumed_self` cases:
    // the `with_*` builders consume the receiver, so returning `self`
    // would hand the caller a now-Rust-owned cdata. Auto-unwrap returns
    // (Option/Result) are non-void so they bypass this entirely.
    let returns_self = func.return_type.is_none() && !consumed_self;

    if !has_callback_arg(func) && !has_az_string && !has_refany && unwrap_call.is_none() && !needs_consume {
        if returns_self {
            out.push_str(&format!(
                "    function {}_methods:{}(...) C.{}(self, ...); return self end\n",
                class, lua_method, func.c_name
            ));
        } else {
            out.push_str(&format!(
                "    function {}_methods:{}(...) return {} end\n",
                class,
                lua_method,
                lua_arm(&format!("C.{}(self, ...)", func.c_name), &ret_fin)
            ));
        }
        return;
    }

    if !has_callback_arg(func) && !has_az_string && !has_refany && !needs_consume {
        // Auto-unwrap only path: keep the varargs varadic, wrap the return.
        // `returns_self` is impossible here (unwrap_call only triggers on
        // Option/Result, which are non-void), so no chainable branch.
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

    emit_arg_coercions(out, "        ", &func.args[1..], &visible, ir);

    // Functions with a callback-wrapper arg call the `<c_name>Struct`
    // C symbol (whole wrapper struct by value; declared in the cdef via
    // the C header's Struct-variant emit). The raw `<c_name>` takes a
    // bare fn ptr at the C ABI, which would drop the host-handle ctx.
    let c_symbol = super::super::managed_host_invoker::managed_c_symbol(func);

    let mut call_args = vec!["self".to_string()];
    for (i, _a) in func.args.iter().skip(1).enumerate() {
        call_args.push(visible[i].clone());
    }
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
                c_symbol,
                call_args.join(", "),
                uw
            )),
            None if returns_self => {
                // CC-6: void-return mutator (no consume, no unwrap). Emit
                // the call as a statement, then return self for chaining.
                out.push_str(&format!(
                    "        C.{}({})\n",
                    c_symbol,
                    call_args.join(", ")
                ));
                out.push_str("        return self\n");
            }
            None => out.push_str(&format!(
                "        return {}\n",
                lua_arm(
                    &format!("C.{}({})", c_symbol, call_args.join(", ")),
                    &ret_fin
                )
            )),
        }
    } else {
        // Multi-line: capture, consume, return.
        match unwrap_call {
            Some(uw) => out.push_str(&format!(
                "        local _ret = (C.{}({})){}\n",
                c_symbol,
                call_args.join(", "),
                uw
            )),
            None if returns_self => {
                // CC-6: void-return mutator with consume lines. Skip the
                // `_ret` capture entirely; emit the call as a statement,
                // run the consume lines, then return self for chaining.
                out.push_str(&format!(
                    "        C.{}({})\n",
                    c_symbol,
                    call_args.join(", ")
                ));
            }
            None => out.push_str(&format!(
                "        local _ret = {}\n",
                lua_arm(
                    &format!("C.{}({})", c_symbol, call_args.join(", ")),
                    &ret_fin
                )
            )),
        }
        for line in &consume_lines {
            out.push_str(line);
            out.push('\n');
        }
        if returns_self {
            out.push_str("        return self\n");
        } else {
            out.push_str("        return _ret\n");
        }
    }
    out.push_str("    end\n");
}

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

/// The `C.Az<T>_delete` finalizer expression to arm on an owned by-value
/// C return of type `ty`, or `None` when the type is Copy (no `_delete`
/// export) or not an IR wrapper type (primitives / pointers / void).
///
/// Rationale: LuaJIT arms `ffi.metatype` `__gc` only for cdata created
/// by `ffi.new` / ctype constructors — aggregates RETURNED BY VALUE from
/// C calls are boxed WITHOUT a finalizer, so without explicit
/// `ffi.gc(v, C.Az<T>_delete)` arming every builder/factory return leaks
/// (verified empirically on LuaJIT 2.1, 2026-07-04).
fn lua_finalizer_for(ty: &str, ir: &CodegenIR) -> Option<String> {
    let t = ty.trim();
    if lua_has_delete(t, ir) {
        Some(format!("C.Az{}_delete", t))
    } else {
        None
    }
}

/// Wrap `expr` in `ffi.gc(expr, <finalizer>)` when a finalizer applies.
fn lua_arm(expr: &str, fin: &Option<String>) -> String {
    match fin {
        Some(f) => format!("ffi.gc({}, {})", expr, f),
        None => expr.to_string(),
    }
}

/// Owned-by-value args of deletable types: the C call transfers their
/// bytes to Rust, so the (possibly armed) cdata must have its finalizer
/// detached afterwards via `azul._consume`. Copy types and primitives
/// are never armed, so they need no consume; owned `RefAny` args are
/// coerced into unarmed transients by `azul._refany_arg` and are exempt
/// too (see `emit_arg_coercions`).
fn lua_consumable_arg_indices(func: &FunctionDef, ir: &CodegenIR) -> Vec<usize> {
    func.args
        .iter()
        .enumerate()
        .filter(|(_, a)| {
            matches!(a.ref_kind, ArgRefKind::Owned)
                && lua_has_delete(a.type_name.trim(), ir)
                && !is_refany_type(&a.type_name, ir)
        })
        .map(|(i, _)| i)
        .collect()
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
fn emit_lua_to_opt_body(out: &mut String, class: &str, payload_ty: &str, ir: &CodegenIR) {
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
        out.push_str("            __out = ffi.string(__azs.vec.ptr, _tonum(__azs.vec.len))\n");
        out.push_str("        end\n");
        if has_delete {
            out.push_str(&delete_line);
        }
        out.push_str("        return __out\n");
    } else if lua_has_clone(payload_ty, ir) {
        // Wrapper / cloneable payload — clone for an independent
        // allocation (armed: it is an owned C-returned value), then
        // delete the Option (drops the original payload's heap
        // allocations).
        out.push_str(&format!(
            "        local __cloned = {}\n",
            lua_arm(
                &format!("C.Az{}_clone(self.Some.payload)", payload_ty),
                &lua_finalizer_for(payload_ty, ir)
            )
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
fn emit_lua_unwrap_body(out: &mut String, class: &str, payload_ty: &str, ir: &CodegenIR) {
    let result_name = format!("Az{}", class);
    let has_delete = lua_has_delete(class, ir);
    // `azul._consume(self)` after the explicit delete: Result shells
    // returned from static wrappers are ARMED, so without disarming
    // the finalizer would re-run Az<R>_delete on the freed shell.
    let delete_line = if has_delete {
        format!(
            "        C.{}_delete(self)\n        azul._consume(self)\n",
            result_name
        )
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
        out.push_str("            __out = ffi.string(__azs.vec.ptr, _tonum(__azs.vec.len))\n");
        out.push_str("        end\n");
        if has_delete {
            out.push_str(&delete_line);
        }
        out.push_str("        return __out\n");
    } else if lua_has_clone(payload_ty, ir) {
        out.push_str(&format!(
            "        local __cloned = {}\n",
            lua_arm(
                &format!("C.Az{}_clone(self.Ok.payload)", payload_ty),
                &lua_finalizer_for(payload_ty, ir)
            )
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

/// Auto-string-conversion rule (mirrors Java/Kotlin/C#/Ruby/Node): any
/// owned `String` arg (IR category `TypeCategory::String`, not a name
/// match) accepts a plain Lua string at the wrapper level; the call site
/// routes it through `azul._az_string` (lang_lua/managed.rs). Type-driven;
/// no method-name allowlist.
fn is_az_string_owned_arg(a: &FunctionArg, ir: &CodegenIR) -> bool {
    lua_payload_is_string(a.type_name.trim(), ir) && matches!(a.ref_kind, ArgRefKind::Owned)
}

/// Owned `RefAny` args (IR category `TypeCategory::RefAny`) accept any Lua
/// value (auto-wrapped into a host-handle RefAny) or an existing RefAny
/// cdata (cloned); see `azul._refany_arg`. Borrowed RefAny args
/// (`&RefAny`) are passed through untouched.
fn is_owned_refany_arg(a: &FunctionArg, ir: &CodegenIR) -> bool {
    is_refany_type(&a.type_name, ir) && matches!(a.ref_kind, ArgRefKind::Owned)
}

/// Pre-call coercion of the visible args — ONE emitter for the three
/// wrapper shapes (instance method, enumerated static, callback static):
/// owned `String` args accept plain Lua strings (`azul._az_string`), owned
/// `RefAny` args accept any Lua value (`azul._refany_arg`). Both helpers
/// return UNARMED transients that the C call consumes, so neither needs a
/// post-call `azul._consume`; the value is reassigned onto the visible
/// variable so the (defensive) consume line emitted for owned deletable
/// args sees the same cdata the C call consumed.
fn emit_arg_coercions(
    out: &mut String,
    indent: &str,
    args: &[FunctionArg],
    names: &[String],
    ir: &CodegenIR,
) {
    for (i, a) in args.iter().enumerate() {
        if is_az_string_owned_arg(a, ir) {
            out.push_str(&format!("{indent}{n} = azul._az_string({n})\n", n = names[i]));
        } else if is_owned_refany_arg(a, ir) {
            out.push_str(&format!("{indent}{n} = azul._refany_arg({n})\n", n = names[i]));
        }
    }
}

/// Emit one entry of a static-method table:
///     method = function(args) ... end,
///
/// `ir` is threaded through so the WCO smart-factory branch can pull
/// the splice path (`default_c_name` + nested `field_path`) from
/// [`layout_callback_factory_info`] instead of hardcoding
/// `AzWindowCreateOptions_default` / `window_state.layout_callback`.
fn emit_static_method(out: &mut String, lua_method: &str, func: &FunctionDef, ir: &CodegenIR) {
    let lua_method = sanitize_lua_ident(lua_method);

    // Worker-thread callback guard: Lua is a single-threaded VM with no
    // runtime lock — libazul would invoke the Lua function on a worker
    // thread and corrupt the VM (see `takes_off_main_thread_callback`).
    // Emit a hard error instead of a VM-corrupting registration.
    if takes_off_main_thread_callback(func) {
        out.push_str(&format!(
            "    {} = function(...)\n\x20       error('ThreadCallback from Lua is unsupported; \
             use the writeback pattern', 2)\n\x20   end,\n",
            lua_method
        ));
        return;
    }

    // When the func has Owned `String` args, switch from the varargs
    // passthrough to an enumerated form so we can route each through
    // `azul._az_string`.
    let has_az_string = func.args.iter().any(|a| is_az_string_owned_arg(a, ir));
    let has_refany = func.args.iter().any(|a| is_owned_refany_arg(a, ir));

    // Ownership plumbing (mirrors emit_instance_method):
    // * armed return  — owned by-value C returns never see the metatype __gc (LuaJIT arms it only
    //   for ffi.new cdata), so arm explicitly.
    // * consumed args — owned by-value args of deletable types transfer their bytes to Rust; disarm
    //   the (possibly armed) cdata after the call so its finalizer can't double-free Rust-owned
    //   memory.
    let ret_fin = func
        .return_type
        .as_deref()
        .and_then(|t| lua_finalizer_for(t, ir));
    let consumable = lua_consumable_arg_indices(func, ir);

    if !has_callback_arg(func) && !has_az_string && !has_refany && consumable.is_empty() {
        out.push_str(&format!(
            "    {} = function(...) return {} end,\n",
            lua_method,
            lua_arm(&format!("C.{}(...)", func.c_name), &ret_fin)
        ));
        return;
    }

    if !has_callback_arg(func) {
        // Enumerated form: auto-string pre-assignment + post-call
        // consume of every owned deletable arg + armed return. The
        // string temps are reassigned onto the visible variable so the
        // consume line disarms the SAME cdata the C call consumed.
        let visible: Vec<String> = func
            .args
            .iter()
            .map(|a| sanitize_lua_ident(&a.name))
            .collect();
        out.push_str(&format!(
            "    {} = function({})\n",
            lua_method,
            visible.join(", ")
        ));
        emit_arg_coercions(out, "        ", &func.args, &visible, ir);
        let call = format!("C.{}({})", func.c_name, visible.join(", "));
        if func.return_type.is_none() {
            out.push_str(&format!("        {}\n", call));
            for i in &consumable {
                out.push_str(&format!("        azul._consume({})\n", visible[*i]));
            }
        } else {
            out.push_str(&format!(
                "        local _ret = {}\n",
                lua_arm(&call, &ret_fin)
            ));
            for i in &consumable {
                out.push_str(&format!("        azul._consume({})\n", visible[*i]));
            }
            out.push_str("        return _ret\n");
        }
        out.push_str("    end,\n");
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
    if cb_args.len() == 1
        && func.args.iter().all(|a| {
            a.callback_info.is_some() || matches!(a.ref_kind, super::super::ir::ArgRefKind::Owned)
        })
    {
        let cb = cb_args[0].callback_info.as_ref().unwrap();
        let wrapper_name = cb.callback_wrapper_name.as_str();
        let returns_self_wrapper = func
            .return_type
            .as_deref()
            .map(|t| t.trim() == wrapper_name)
            .unwrap_or(false);
        if returns_self_wrapper && func.args.len() == 1 {
            // Direct passthrough — the registered wrapper IS the return.
            let arg_name = sanitize_lua_ident(&func.args[0].name);
            out.push_str(&format!("    {} = function({})\n", lua_method, arg_name));
            out.push_str(&format!(
                "        return azul._register_callback('{}', {})\n",
                wrapper_name, arg_name
            ));
            out.push_str("    end,\n");
            return;
        }
    }

    // Special-case 2: smart constructor that takes a registered-host
    // callback and splices it into a nested field of the returned
    // class. Detection + splice metadata both come from
    // [`layout_callback_factory_info`] — `class_name`,
    // `default_c_name`, `callback_wrapper`, and `field_path` are all
    // IR-derived. The raw `C.<class>_create(<cb_type>)` path discards
    // the host-invoker ctx; this branch routes through
    // `<default>_default()` + nested field assignment instead, so the
    // ctx survives.
    //
    // Trigger here is a structural match on `func` against the class's
    // factory info (looked up via `ir`); the body is fully driven by
    // the resulting `LayoutCallbackFactoryInfo`.
    if let Some(info) = ir
        .find_struct(&func.class_name)
        .and_then(|s| super::super::managed_host_invoker::layout_callback_factory_info(s, ir))
    {
        let returns_self = func
            .return_type
            .as_deref()
            .map(|r| r.trim() == func.class_name)
            .unwrap_or(false);
        let arg_matches_factory = func.args.len() == 1
            && cb_args.len() == 1
            && cb_args[0]
                .callback_info
                .as_ref()
                .map(|c| c.callback_wrapper_name == info.callback_wrapper)
                .unwrap_or(false);
        if returns_self && arg_matches_factory {
            let arg_name = sanitize_lua_ident(&func.args[0].name);
            out.push_str(&format!("    {} = function({})\n", lua_method, arg_name));
            out.push_str(&format!(
                "        local _cb = azul._register_callback('{}', {})\n",
                info.callback_wrapper, arg_name
            ));
            // `_opts` is an owned C-returned value — arm its finalizer.
            // `_cb` is deliberately NOT armed (callback wrapper structs
            // from `_register_callback` are process-lifetime pins; see
            // managed.rs), so the byte-splice below creates no
            // double-free pair.
            out.push_str(&format!(
                "        local _opts = {}\n",
                lua_arm(
                    &format!("C.{}()", info.default_c_name),
                    &lua_finalizer_for(&func.class_name, ir)
                )
            ));
            out.push_str(&format!(
                "        _opts.{} = _cb\n",
                info.field_path.join(".")
            ));
            out.push_str("        return _opts\n");
            out.push_str("    end,\n");
            return;
        }
    }

    // Open: 4-space indent (inside the table literal).
    out.push_str(&format!(
        "    {} = function({})\n",
        lua_method,
        visible.join(", ")
    ));

    // Body: 8-space indent.
    emit_callback_pin_lines(out, "        ", &func.args[..], &visible);

    emit_arg_coercions(out, "        ", &func.args, &visible, ir);
    // Functions with a callback-wrapper arg call the `<c_name>Struct`
    // C symbol (whole wrapper struct by value); see emit_method.
    let call = format!(
        "C.{}({})",
        super::super::managed_host_invoker::managed_c_symbol(func),
        visible.join(", ")
    );
    if func.return_type.is_none() {
        out.push_str(&format!("        {}\n", call));
        for i in &consumable {
            out.push_str(&format!("        azul._consume({})\n", visible[*i]));
        }
    } else {
        out.push_str(&format!(
            "        local _ret = {}\n",
            lua_arm(&call, &ret_fin)
        ));
        for i in &consumable {
            out.push_str(&format!("        azul._consume({})\n", visible[*i]));
        }
        out.push_str("        return _ret\n");
    }
    out.push_str("    end,\n");
}

/// Emit a callback-arg coercion line for every callback-typed entry in
/// `args`: the user's Lua function goes to `azul._register_callback(<kind>,
/// fn)`, which builds the kind's wrapper struct through libazul's
/// `_createFromHostHandle` (every callback wrapper has one). What replaces the
/// variable depends on the argument's type:
///
///   * the wrapper struct (every callback argument of an API function, see
///     `ir_builder::build_function_def`): the WHOLE struct - the call site binds
///     the `<c_name>Struct` export (`managed_c_symbol`), so the host handle in
///     its context survives the C boundary;
///   * the function-pointer typedef (a wrapper's own constructor, which builds
///     the wrapper from it): just `.cb`.
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
        let abi_takes_wrapper = a.type_name.trim() == wrapper_name;
        out.push_str(&format!(
            "{indent}local _{n}_cb = azul._register_callback('{w}', {n})\n",
            indent = indent,
            n = names[i],
            w = wrapper_name
        ));
        let field = if abi_takes_wrapper { "" } else { ".cb" };
        out.push_str(&format!(
            "{indent}{n} = _{n}_cb{field}\n",
            indent = indent,
            n = names[i],
        ));
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

// ============================================================================
// Static Lua chunks
// ============================================================================

/// The derive surface shared by every wrapper type, emitted once at the top
/// of the wrapper section and called from each type's `ffi.metatype` line
/// (see [`emit_metatype`]).
///
/// One helper rather than four closures written out per type: the metamethod
/// bodies are identical for all ~1 900 types, and the main chunk pays one
/// constant-table entry per type for the `syms` literal instead of a
/// function prototype per metamethod.
const DERIVE_HELPER: &str = r#"-- ------------------------------------------------------------------
-- Derive surface shared by every wrapper type
-- ------------------------------------------------------------------
--
-- Equality, ordering, hashing, the debug string and ownership all route
-- through the C export libazul generated from the Rust `derive`, so
-- `a == b`, `a < b` and `a:hash()` answer exactly what Rust's PartialEq /
-- Ord / Hash say about the same two values -- and equal values hash equal,
-- which a Lua-side reimplementation could never promise.
--
-- Symbols arrive BY NAME, never as `C.AzFoo_cmp`: naming ~6 000 comparators
-- at module load would `ffi.cdef` every one of them and blow LuaJIT's
-- 16-bit ctype budget (see the lazy `__az_fn_decls` registry in the
-- prologue). `C[sym]` inside a closure pays the memoizing proxy once, on
-- the first comparison, and is a plain table read afterwards.
--
-- `syms` keys, each present iff libazul exports that derive for the type:
--   delete  Drop       -> __gc
--   clone   Clone      -> :clone()
--   eq      PartialEq  -> __eq
--   cmp     Ord        -> :cmp(other), __lt, __le
--   pcmp    PartialOrd -> :partial_cmp(other), and the ordering
--                         metamethods when there is no total order
--   hash    Hash       -> :hash()
--   dbg     Debug      -> :toString(), __tostring
--   len     the ptr/len/cap layout -> #value
--   str     the string type        -> __tostring is the text itself
local function _az_derive(methods, ctname, syms)
    local ct = ffi.typeof(ctname)
    local mt = { __index = methods }
    local delete = syms.delete

    -- Ownership: both FFIs run __gc only for cdata that `ffi.new` created,
    -- so this frees values the user constructs and never a struct field or
    -- a C-call return -- those are armed explicitly with `ffi.gc` at the
    -- wrapper boundary and disarmed by `azul._consume` once a C call has
    -- taken the bytes over.
    if delete then
        mt.__gc = function(self) C[delete](self) end
    end

    -- A clone is a fresh owned value returned BY VALUE, which the FFI hands
    -- back unarmed; arm it so dropping it frees it exactly once.
    if syms.clone then
        local clone = syms.clone
        if delete then
            methods.clone = function(self) return ffi.gc(C[clone](self), C[delete]) end
        else
            methods.clone = function(self) return C[clone](self) end
        end
    end

    -- uint64_t: a boxed integer cdata on both runtimes, returned whole so
    -- no bits are lost. `tostring(v:hash())` is the portable table key.
    if syms.hash then
        local hash = syms.hash
        methods.hash = function(self) return C[hash](self) end
    end

    if syms.eq then
        local eq = syms.eq
        -- Both FFIs invoke __eq with `nil` or a foreign cdata as the second
        -- operand; reading those bytes as this type would compare garbage,
        -- so anything that is not this type is simply not equal to it.
        -- `ffi.istype` (never `type(x) == 'cdata'`) keeps the check exact
        -- under cffi-lua, where cdata are userdata to `type()`.
        mt.__eq = function(a, b)
            if not (ffi.istype(ct, a) and ffi.istype(ct, b)) then return false end
            return C[eq](a, b)
        end
    end

    -- The comparators answer 0 = Less, 1 = Equal, 2 = Greater, and
    -- _partialCmp additionally 255 = incomparable (a NaN inside the value):
    -- the encoding the DLL emits for core::cmp::Ordering.
    if syms.cmp then
        local cmp = syms.cmp
        methods.cmp = function(self, other) return C[cmp](self, other) end
    end
    if syms.pcmp then
        local pcmp = syms.pcmp
        methods.partial_cmp = function(self, other)
            local o = C[pcmp](self, other)
            if o == 255 then return nil end -- Rust's `None`: not comparable
            return o
        end
    end
    -- A total order answers everything `<` and `<=` can ask. A partial one
    -- answers 255 for a NaN, which is neither `<` nor `<=`, like Rust.
    local order = syms.cmp or syms.pcmp
    if order then
        local function ordered(a, b)
            if not (ffi.istype(ct, a) and ffi.istype(ct, b)) then
                error('azul: < and <= need two ' .. ctname .. ' values', 3)
            end
            return C[order](a, b)
        end
        mt.__lt = function(a, b) return ordered(a, b) == 0 end
        mt.__le = function(a, b) local o = ordered(a, b); return o == 0 or o == 1 end
    end

    if syms.dbg then
        local dbg = syms.dbg
        -- :toString() hands back the AzString itself, armed -- the caller
        -- owns it. __tostring copies the bytes into a Lua string and frees
        -- the AzString, which no finalizer would ever see (a C-call return
        -- is never armed by the FFI), so printing a value cannot leak.
        methods.toString = function(self) return ffi.gc(C[dbg](self), C.AzString_delete) end
        if not syms.str then
            mt.__tostring = function(self)
                local az = C[dbg](self)
                local ok = az.vec.ptr ~= nil and az.vec.len > 0
                local s = ok and ffi.string(az.vec.ptr, _tonum(az.vec.len)) or ''
                C.AzString_delete(az)
                return s
            end
        end
    end

    -- The string type prints as its own text, not as its Debug form.
    if syms.str then
        mt.__tostring = function(self)
            if self.vec.ptr == nil or self.vec.len == 0 then return '' end
            return ffi.string(self.vec.ptr, _tonum(self.vec.len))
        end
    end

    if syms.len then
        mt.__len = function(self) return _tonum(self.len) end
    end

    return mt
end

"#;
