//! Common utilities for C++ code generation
//!
//! This module provides shared helper functions used by all C++ dialect generators.

use super::super::config::*;
use super::super::ir::*;

// ============================================================================
// C++ Reserved Keywords
// ============================================================================

/// C++ reserved keywords that need to be escaped by appending underscore
pub const CPP_RESERVED_KEYWORDS: &[&str] = &[
    // gcc/clang predefine `linux`/`unix` as macros on those platforms — escape so
    // a field named `linux` doesn't expand to `1` (kept in sync with lang_c).
    "linux",
    "unix",
    // windows.h (pulled in via azul.h on MSVC/MinGW) #defines these object-like
    // macros, so an arg named e.g. `near` expands to nothing -> "expected
    // expression" (hit by GlContextPtr::depth_range(double near, double far)).
    // Escape them so the generated C++ params don't collide.
    "near",
    "far",
    "small",
    "min",
    "max",
    "alignas",
    "alignof",
    "and",
    "and_eq",
    "asm",
    "auto",
    "bitand",
    "bitor",
    "bool",
    "break",
    "case",
    "catch",
    "char",
    "char8_t",
    "char16_t",
    "char32_t",
    "class",
    "compl",
    "concept",
    "const",
    "consteval",
    "constexpr",
    "constinit",
    "const_cast",
    "continue",
    "co_await",
    "co_return",
    "co_yield",
    "decltype",
    "default",
    "delete",
    "do",
    "double",
    "dynamic_cast",
    "else",
    "enum",
    "explicit",
    "export",
    "extern",
    "false",
    "float",
    "for",
    "friend",
    "goto",
    "if",
    "inline",
    "int",
    "long",
    "mutable",
    "namespace",
    "new",
    "noexcept",
    "not",
    "not_eq",
    "nullptr",
    "operator",
    "or",
    "or_eq",
    "private",
    "protected",
    "public",
    "reflexpr",
    "register",
    "reinterpret_cast",
    "requires",
    "return",
    "short",
    "signed",
    "sizeof",
    "static",
    "static_assert",
    "static_cast",
    "struct",
    "switch",
    "synchronized",
    "template",
    "this",
    "thread_local",
    "throw",
    "true",
    "try",
    "typedef",
    "typeid",
    "typename",
    "union",
    "unsigned",
    "using",
    "virtual",
    "void",
    "volatile",
    "wchar_t",
    "while",
    "xor",
    "xor_eq",
];

/// Escape C++ reserved keywords by appending an underscore
pub fn escape_cpp_keyword(name: &str) -> String {
    if CPP_RESERVED_KEYWORDS.contains(&name) {
        format!("{}_", name)
    } else {
        name.to_string()
    }
}

/// Escape method name (handles new, default which are reserved)
pub fn escape_method_name(name: &str) -> String {
    match name {
        "new" => "new_".to_string(),
        "default" => "default_".to_string(),
        _ => escape_cpp_keyword(name),
    }
}

// ============================================================================
// Function Classification Helpers
// ============================================================================

/// Check if a function is a constructor (including Default trait)
pub fn is_constructor_or_default(func: &FunctionDef) -> bool {
    matches!(func.kind, FunctionKind::Constructor) || func.kind.is_default_constructor()
}

/// Check if a function is a method that should be skipped (delete, partialEq, etc.)
/// Note: DeepCopy is NOT skipped - it becomes clone()
/// Note: Default is NOT skipped - it becomes default_()
pub fn should_skip_method(func: &FunctionDef) -> bool {
    matches!(func.kind, FunctionKind::Constructor) ||
    func.kind.is_trait_function() ||  // delete, partialEq, etc. - but NOT deepCopy or default
    func.kind.is_default_constructor() // handled separately as static constructor
}

/// Check if callback substitution should be applied for a function
/// True for any "user-facing" function (constructors, instance methods both
/// const and mut, static methods); skipped for trait-generated functions and
/// enum-variant constructors.
pub fn should_substitute_callbacks(func: &FunctionDef) -> bool {
    matches!(
        func.kind,
        FunctionKind::Constructor
            | FunctionKind::Method
            | FunctionKind::MethodMut
            | FunctionKind::StaticMethod
    ) || func.kind.is_default_constructor()
}

// ============================================================================
// Primitive Type Handling
// ============================================================================

/// Check if a type name is a primitive type
pub fn is_primitive(type_name: &str) -> bool {
    matches!(
        type_name,
        "bool" | "u8" | "u16" | "u32" | "u64" | "usize" |
        "i8" | "i16" | "i32" | "i64" | "isize" |
        "f32" | "f64" | "c_void" | "()" |
        // C FFI types
        "c_int" | "c_uint" | "c_long" | "c_ulong" |
        "c_char" | "c_uchar" | "c_short" | "c_ushort" |
        "c_longlong" | "c_ulonglong" | "c_float" | "c_double"
    )
}

/// Convert Rust primitive type to C type
pub fn primitive_to_c(type_name: &str) -> String {
    match type_name {
        "bool" => "bool".to_string(),
        "u8" => "uint8_t".to_string(),
        "u16" => "uint16_t".to_string(),
        "u32" => "uint32_t".to_string(),
        "u64" => "uint64_t".to_string(),
        "usize" => "size_t".to_string(),
        "i8" => "int8_t".to_string(),
        "i16" => "int16_t".to_string(),
        "i32" => "int32_t".to_string(),
        "i64" => "int64_t".to_string(),
        "isize" => "ptrdiff_t".to_string(),
        "f32" => "float".to_string(),
        "f64" => "double".to_string(),
        "c_void" | "()" => "void".to_string(),
        // C FFI types
        "c_char" => "char".to_string(),
        "c_uchar" => "unsigned char".to_string(),
        "c_short" => "short".to_string(),
        "c_ushort" => "unsigned short".to_string(),
        "c_int" => "int".to_string(),
        "c_uint" => "unsigned int".to_string(),
        "c_long" => "long".to_string(),
        "c_ulong" => "unsigned long".to_string(),
        "c_longlong" => "long long".to_string(),
        "c_ulonglong" => "unsigned long long".to_string(),
        "c_float" => "float".to_string(),
        "c_double" => "double".to_string(),
        _ => type_name.to_string(),
    }
}

// ============================================================================
// Type Classification
// ============================================================================

/// Check if a struct is a Vec type (has ptr, len, cap, destructor fields)
pub fn is_vec_type(struct_def: &StructDef) -> bool {
    matches!(struct_def.category, TypeCategory::Vec)
}

/// Check if a struct is a String type
pub fn is_string_type(struct_def: &StructDef) -> bool {
    matches!(struct_def.category, TypeCategory::String)
}

/// Check if a struct is an Option type
pub fn is_option_type(struct_def: &StructDef) -> bool {
    matches!(struct_def.category, TypeCategory::Option)
}

/// Check if a struct is a Result type
pub fn is_result_type(struct_def: &StructDef) -> bool {
    matches!(struct_def.category, TypeCategory::Result)
}

/// Check if an enum is an Option type
pub fn is_option_enum(enum_def: &EnumDef) -> bool {
    matches!(enum_def.category, TypeCategory::Option)
}

/// Check if an enum is a Result type
pub fn is_result_enum(enum_def: &EnumDef) -> bool {
    matches!(enum_def.category, TypeCategory::Result)
}

/// For an `OptionXxx` enum, return the inner type carried by the `Some`
/// variant. Returns `None` if the enum is not an Option or has no payload.
pub fn get_option_inner_from_enum(enum_def: &EnumDef) -> Option<String> {
    if !is_option_enum(enum_def) {
        return None;
    }
    let some = enum_def.variants.iter().find(|v| v.name == "Some")?;
    if let EnumVariantKind::Tuple(payload) = &some.kind {
        payload.first().map(|(t, _)| t.clone())
    } else {
        None
    }
}

/// For a `ResultXxx` enum, return the (Ok, Err) payload types.
/// Returns `None` if the enum is not a Result or is missing Ok/Err.
pub fn get_result_payload_types(enum_def: &EnumDef) -> Option<(String, String)> {
    if !is_result_enum(enum_def) {
        return None;
    }
    let ok = enum_def.variants.iter().find(|v| v.name == "Ok")?;
    let err = enum_def.variants.iter().find(|v| v.name == "Err")?;
    let ok_t = if let EnumVariantKind::Tuple(p) = &ok.kind {
        p.first().map(|(t, _)| t.clone())?
    } else {
        return None;
    };
    let err_t = if let EnumVariantKind::Tuple(p) = &err.kind {
        p.first().map(|(t, _)| t.clone())?
    } else {
        return None;
    };
    Some((ok_t, err_t))
}

/// Emit non-prefixed value constants for a unit (non-union) enum, scoped in
/// a namespace so user code can write `Update::RefreshDom` (the same spelling
/// Rust uses) while every constant keeps the raw C enum type (`AzUpdate`) and
/// therefore stays implicitly usable wherever the C API expects the enum —
/// including as the return value of a raw fn-ptr callback.
///
/// This intentionally replaces the old `using Update = AzUpdate;` type alias:
/// a namespace and a type cannot share a name in the same scope, and the
/// constants are the part user code actually spells. The C type name
/// (`AzUpdate`) remains available for signatures, and every existing
/// `AzUpdate_RefreshDom` constant spelling keeps working.
///
/// `const_decl` is the per-standard variable form:
///   - C++03:    "static const"      (internal linkage, constant-initialized)
///   - C++11/14: "constexpr"         (internal linkage per TU — header-safe)
///   - C++17+:   "inline constexpr"  (one entity across TUs)
pub fn generate_enum_constants_namespace(
    enum_def: &EnumDef,
    config: &CodegenConfig,
    const_decl: &str,
) -> String {
    let enum_name = &enum_def.name;
    let c_type_name = config.apply_prefix(enum_name);
    let mut code = String::new();
    // Suppressed when this header is #included as the global-module fragment
    // of azul.cppm (which #defines AZUL_MODULE_EXPORT): the module re-declares
    // these constants in its own exported purview so `import azul;` consumers
    // see `azul::Update::RefreshDom`. A module-attached re-declaration would
    // collide with a global-fragment one, hence the guard.
    code.push_str("#ifndef AZUL_MODULE_EXPORT\r\n");
    code.push_str(&format!("namespace {} {{\r\n", enum_name));
    for variant in &enum_def.variants {
        code.push_str(&format!(
            "    {} {} {} = {}_{};\r\n",
            const_decl, c_type_name, variant.name, c_type_name, variant.name
        ));
    }
    code.push_str(&format!("}} // namespace {}\r\n", enum_name));
    code.push_str("#endif // AZUL_MODULE_EXPORT\r\n\r\n");
    code
}

/// ODR-safe variant of [`generate_enum_constants_namespace`] for the
/// pre-C++17 dialects (C++03/11/14), where namespace-scope `const` /
/// `constexpr` constants have INTERNAL linkage — each translation unit gets
/// its own copy, so `&Update::RefreshDom` differs between TUs and taking that
/// address inside a cross-TU `inline` function is an ODR violation.
///
/// The fix is the standard header-only external-linkage-constant idiom: a
/// class *template*'s static data members have external linkage and the
/// linker merges them to a single definition across TUs (templates are exempt
/// from the one-definition rule for their own definitions). `Update` becomes a
/// typedef/alias to that template instance, so `Update::RefreshDom` still
/// reads as an `AzUpdate` value *and* has one address program-wide.
///
/// C++17+ keeps the cleaner `inline constexpr` namespace form (inline
/// variables already have external linkage) — see the caller split.
///
/// `is_cpp03` picks `static const` + `typedef` (no `constexpr`/`using` in
/// C++03) vs `static constexpr` + `using` for C++11/14.
pub fn generate_enum_constants_extern(
    enum_def: &EnumDef,
    config: &CodegenConfig,
    is_cpp03: bool,
) -> String {
    let enum_name = &enum_def.name;
    let c_type_name = config.apply_prefix(enum_name);
    let member_kw = if is_cpp03 { "static const" } else { "static constexpr" };
    let def_kw = if is_cpp03 { "const" } else { "constexpr" };
    let holder = format!("{}_consts_", enum_name);

    let mut code = String::new();
    // See generate_enum_constants_namespace for the AZUL_MODULE_EXPORT guard.
    code.push_str("#ifndef AZUL_MODULE_EXPORT\r\n");
    code.push_str("namespace az_enum_detail {\r\n");
    // The template parameter is only there to make these static members
    // template members (→ external linkage, mergeable across TUs).
    code.push_str(&format!("template<class = void> struct {} {{\r\n", holder));
    for variant in &enum_def.variants {
        code.push_str(&format!(
            "    {} {} {} = {}_{};\r\n",
            member_kw, c_type_name, variant.name, c_type_name, variant.name
        ));
    }
    code.push_str("};\r\n");
    // Out-of-line definitions so the members are ODR-usable (address-taking,
    // reference binding) — one line each, still header-only via the template.
    for variant in &enum_def.variants {
        code.push_str(&format!(
            "template<class T> {} {} {}<T>::{};\r\n",
            def_kw, c_type_name, holder, variant.name
        ));
    }
    code.push_str("} // namespace az_enum_detail\r\n");
    if is_cpp03 {
        code.push_str(&format!(
            "typedef az_enum_detail::{}<> {};\r\n\r\n",
            holder, enum_name
        ));
    } else {
        code.push_str(&format!(
            "using {} = az_enum_detail::{}<>;\r\n",
            enum_name, holder
        ));
    }
    code.push_str("#endif // AZUL_MODULE_EXPORT\r\n\r\n");
    code
}

/// Emit non-prefixed aliases for the raw C callback fn-ptr typedefs
/// (`using LayoutCallbackType = AzLayoutCallbackType;`). The callback
/// registration methods take these raw fn-ptr types, so aliasing them lets
/// the generated method signatures (and user code holding fn ptrs) drop the
/// `Az` prefix. Sorted by name so the output is stable across regens.
/// `use_typedef` switches to C++03 `typedef` syntax.
pub fn generate_callback_typedef_aliases(
    ir: &CodegenIR,
    config: &CodegenConfig,
    use_typedef: bool,
) -> String {
    let mut cbs: Vec<&CallbackTypedefDef> = ir
        .callback_typedefs
        .iter()
        .filter(|cb| config.should_include_type(&cb.name))
        .collect();
    cbs.sort_by(|a, b| a.name.cmp(&b.name));
    if cbs.is_empty() {
        return String::new();
    }
    let mut code = String::new();
    code.push_str("// Callback fn-ptr typedef aliases. These ARE the C types, so user\r\n");
    code.push_str("// callbacks must still be defined with the raw C parameter structs\r\n");
    code.push_str("// (function-pointer types have to match the C signature exactly).\r\n");
    for cb in cbs {
        let c_name = config.apply_prefix(&cb.name);
        if use_typedef {
            code.push_str(&format!("typedef {} {};\r\n", c_name, cb.name));
        } else {
            code.push_str(&format!("using {} = {};\r\n", cb.name, c_name));
        }
    }
    code.push_str("\r\n");
    code
}

/// Emit `toStdOptional()` (+ the implicit `operator std::optional<T>`) for an
/// Option wrapper class (C++17 and later). When the payload has a
/// non-prefixed wrapper class, the conversion yields
/// `std::optional<Wrapper>` instead of the raw C payload struct:
///   - Copy payloads: `const`-qualified, wraps a bitwise copy (the wrapper
///     for a Copy type is itself copyable and owns nothing).
///   - Non-copy payloads: `&&`-qualified (consuming) — ownership of the
///     payload transfers into the wrapper and the Option resets to None,
///     mirroring the C++23 `toStdExpected() &&` shape (no double free).
/// Payloads without a wrapper class keep the historical raw-payload form.
pub fn emit_option_to_std_optional(
    code: &mut String,
    inner_type: &str,
    c_inner_type: &str,
    ir: &CodegenIR,
) {
    if !type_has_wrapper(inner_type, ir) {
        code.push_str(&format!(
            "    std::optional<{}> toStdOptional() const {{ return isSome() ? std::optional<{}>(inner_.Some.payload) : std::nullopt; }}\r\n",
            c_inner_type, c_inner_type
        ));
        code.push_str(&format!(
            "    operator std::optional<{}>() const {{ return toStdOptional(); }}\r\n",
            c_inner_type
        ));
        return;
    }
    let payload_is_copy = ir
        .find_struct(inner_type)
        .map(|s| s.traits.is_copy)
        .unwrap_or(false);
    if payload_is_copy {
        code.push_str(&format!(
            "    std::optional<{w}> toStdOptional() const {{ return isSome() ? std::optional<{w}>({w}(inner_.Some.payload)) : std::nullopt; }}\r\n",
            w = inner_type
        ));
        code.push_str(&format!(
            "    operator std::optional<{w}>() const {{ return toStdOptional(); }}\r\n",
            w = inner_type
        ));
    } else {
        code.push_str(&format!(
            "    std::optional<{w}> toStdOptional() && {{ if (!isSome()) return std::nullopt; {c} v = inner_.Some.payload; inner_ = {{}}; return std::optional<{w}>({w}(v)); }}\r\n",
            w = inner_type,
            c = c_inner_type
        ));
        code.push_str(&format!(
            "    operator std::optional<{w}>() && {{ return std::move(*this).toStdOptional(); }}\r\n",
            w = inner_type
        ));
    }
}

/// Generate the `azul.cppm` module partition file. Emitted alongside
/// `azul20.hpp` / `azul23.hpp` so users on a modules-aware toolchain can
/// `import azul;` instead of `#include "azul20.hpp"`.
///
/// The class-name list comes from iterating `ir.structs` and filtering with
/// the same predicates the header itself uses for forward declarations
/// (`should_skip_class`, `renders_as_type_alias`).
pub fn generate_module_partition(
    ir: &CodegenIR,
    config: &CodegenConfig,
    standard: CppStandard,
) -> String {
    let header_name = standard.header_filename();
    let mut code = String::new();
    code.push_str("// Auto-generated module partition for the Azul C++ wrapper.\r\n");
    code.push_str("// Compile with `c++ -std=c++20 -fmodules-ts -c azul.cppm` (or your toolchain's\r\n");
    code.push_str("// equivalent) so consumers can `import azul;`.\r\n\r\n");
    code.push_str("module;\r\n");
    // Suppress the header's own unit-enum constant namespaces in the global
    // module fragment so we can re-declare them, exported, in the purview
    // below (a module-attached re-declaration can't collide with a
    // global-fragment one). See generate_enum_constants_namespace.
    code.push_str("#define AZUL_MODULE_EXPORT 1\r\n");
    code.push_str(&format!("#include \"{}\"\r\n", header_name));
    code.push_str("export module azul;\r\n\r\n");
    code.push_str("export namespace azul {\r\n");

    // Synthesized Option/Result wrappers also count as exported names.
    let synthesized = synthesize_option_result_structs(ir);
    let mut all_structs: Vec<&StructDef> = ir.structs.iter().collect();
    all_structs.extend(synthesized.iter());
    all_structs.sort_by_key(|s| s.sort_order);

    for struct_def in &all_structs {
        if !config.should_include_type(&struct_def.name) {
            continue;
        }
        if should_skip_class(struct_def) || renders_as_type_alias(struct_def) {
            continue;
        }
        code.push_str(&format!("    using azul::{};\r\n", struct_def.name));
    }
    // RefAny carries the template-reflection API (create<T>, type_id<T>,
    // downcast_ref<T>, downcast_mut<T>) as members. The concept is the only
    // namespace-level entry point that needs an explicit re-export.
    if standard >= CppStandard::Cpp20 {
        code.push_str("    using azul::ReflectableModel;\r\n");
    }

    // Unit-enum value constants (`azul::Update::RefreshDom`). The header's own
    // copies were suppressed via AZUL_MODULE_EXPORT above; re-declare them here
    // in the exported purview so `import azul;` consumers can spell them. The
    // C enumerators (AzUpdate_RefreshDom) come from the global-fragment
    // #include and are visible here. `inline constexpr` (C++20 is guaranteed
    // for the module).
    for enum_def in &ir.enums {
        if enum_def.is_union || !enum_def.generic_params.is_empty() {
            continue;
        }
        if !config.should_include_type(&enum_def.name) {
            continue;
        }
        let c_type_name = config.apply_prefix(&enum_def.name);
        code.push_str(&format!("    namespace {} {{\r\n", enum_def.name));
        for variant in &enum_def.variants {
            code.push_str(&format!(
                "        inline constexpr {ct} {v} = {ct}_{v};\r\n",
                ct = c_type_name,
                v = variant.name
            ));
        }
        code.push_str(&format!("    }} // namespace {}\r\n", enum_def.name));
    }

    code.push_str("} // namespace azul\r\n");
    code
}

/// Emit `std::tuple_size`/`tuple_element` specializations + `get<I>`
/// overloads for every Result-categorized enum so users can write
/// `auto [ok, err] = std::move(result);` in C++17+.
///
/// The block must live outside `namespace azul` because the specializations
/// target `std::tuple_size` / `std::tuple_element`. ADL on the wrapper class
/// finds the `get<I>(azul::ResultXxx&&)` free functions in `namespace azul`.
pub fn generate_structured_binding_specs(ir: &CodegenIR) -> String {
    let mut code = String::new();
    let results: Vec<&EnumDef> = ir
        .enums
        .iter()
        .filter(|e| matches!(e.category, TypeCategory::Result) && e.generic_params.is_empty())
        .collect();
    if results.is_empty() {
        return code;
    }

    code.push_str("// Structured-binding specializations: every ResultXxx wrapper destructures\r\n");
    code.push_str("// to (std::optional<Ok>, std::optional<Err>).\r\n");

    // First emit get<I> ADL hooks inside namespace azul (one block).
    code.push_str("namespace azul {\r\n");
    for enum_def in &results {
        let (ok_t, err_t) = match get_result_payload_types(enum_def) {
            Some(p) => p,
            None => continue,
        };
        let c_ok = if is_primitive(&ok_t) {
            primitive_to_c(&ok_t)
        } else {
            format!("Az{}", ok_t)
        };
        let c_err = if is_primitive(&err_t) {
            primitive_to_c(&err_t)
        } else {
            format!("Az{}", err_t)
        };
        let class_name = &enum_def.name;
        code.push_str(&format!(
            "template<size_t I> auto get({}&& r) {{\r\n",
            class_name
        ));
        code.push_str(&format!(
            "    static_assert(I < 2, \"{} only has 2 elements (ok, err)\");\r\n",
            class_name
        ));
        code.push_str("    if constexpr (I == 0) {\r\n");
        code.push_str(&format!(
            "        return r.isOk() ? std::optional<{ok}>(std::move(r).inner().Ok.payload) : std::optional<{ok}>{{}};\r\n",
            ok = c_ok,
        ));
        code.push_str("    } else {\r\n");
        code.push_str(&format!(
            "        return r.isErr() ? std::optional<{err}>(std::move(r).inner().Err.payload) : std::optional<{err}>{{}};\r\n",
            err = c_err,
        ));
        code.push_str("    }\r\n");
        code.push_str("}\r\n");
    }
    code.push_str("} // namespace azul\r\n\r\n");

    // Now the std:: specializations (must live in `namespace std`).
    for enum_def in &results {
        let (ok_t, err_t) = match get_result_payload_types(enum_def) {
            Some(p) => p,
            None => continue,
        };
        let c_ok = if is_primitive(&ok_t) {
            primitive_to_c(&ok_t)
        } else {
            format!("Az{}", ok_t)
        };
        let c_err = if is_primitive(&err_t) {
            primitive_to_c(&err_t)
        } else {
            format!("Az{}", err_t)
        };
        let class_name = &enum_def.name;
        code.push_str(&format!(
            "template<> struct std::tuple_size<azul::{}> : std::integral_constant<size_t, 2> {{}};\r\n",
            class_name
        ));
        code.push_str(&format!(
            "template<> struct std::tuple_element<0, azul::{}> {{ using type = std::optional<{}>; }};\r\n",
            class_name, c_ok
        ));
        code.push_str(&format!(
            "template<> struct std::tuple_element<1, azul::{}> {{ using type = std::optional<{}>; }};\r\n",
            class_name, c_err
        ));
    }
    code.push_str("\r\n");
    code
}

/// Synthesize a `StructDef` from an Option/Result-categorized `EnumDef`.
/// Lets the existing wrapper-class generation pass handle Option/Result types
/// without a parallel emission path.
///
/// The synthetic struct has empty `fields` (the C-side representation is a
/// `union`, accessed only via `inner_`), and inherits traits/derives/category
/// from the enum.
pub fn synthesize_option_result_structs(ir: &CodegenIR) -> Vec<StructDef> {
    ir.enums
        .iter()
        .filter(|e| matches!(e.category, TypeCategory::Option | TypeCategory::Result))
        .filter(|e| e.generic_params.is_empty())
        .map(|e| StructDef {
            name: e.name.clone(),
            doc: e.doc.clone(),
            fields: Vec::new(),
            external_path: e.external_path.clone(),
            module: e.module.clone(),
            derives: e.derives.clone(),
            has_explicit_derive: e.has_explicit_derive,
            custom_impls: Vec::new(),
            is_boxed: false,
            repr: e.repr.clone(),
            is_send_safe: e.is_send_safe,
            generic_params: Vec::new(),
            traits: e.traits.clone(),
            category: e.category,
            dependencies: e.dependencies.clone(),
            sort_order: e.sort_order,
            needs_forward_decl: e.needs_forward_decl,
            callback_wrapper_info: None,
        })
        .collect()
}

/// Check if a struct is Copy (can be trivially copied)
pub fn is_copy(struct_def: &StructDef) -> bool {
    struct_def.traits.is_copy
}

/// Check if a struct needs a destructor
pub fn needs_destructor(struct_def: &StructDef) -> bool {
    struct_def.traits.needs_delete()
}

/// Check if a struct has the Default trait
pub fn has_default(struct_def: &StructDef) -> bool {
    struct_def.traits.is_default
}

/// Check if a type is a callback wrapper and return its typedef name
/// For types like LayoutCallback, VirtualViewCallback, etc. that have a `cb` field
/// with a CallbackType typedef
pub fn get_callback_typedef_name(type_name: &str, ir: &CodegenIR) -> Option<String> {
    ir.find_struct(type_name)
        .and_then(|s| s.callback_wrapper_info.as_ref())
        .map(|info| info.callback_typedef_name.clone())
}

/// Get the element type of a Vec (from the ptr field)
pub fn get_vec_element_type(struct_def: &StructDef) -> Option<String> {
    struct_def
        .fields
        .iter()
        .find(|f| f.name == "ptr")
        .map(|f| f.type_name.clone())
        .filter(|t| t != "c_void" && t != "void")
}

/// Get the inner type of an Option. Looks up the sibling enum's `Some`
/// variant payload — the prefix-stripping fallback only fires for legacy
/// callers that don't have IR access.
pub fn get_option_inner_type(struct_def: &StructDef) -> Option<String> {
    struct_def
        .name
        .strip_prefix("Option")
        .map(|s| s.to_string())
}

/// IR-aware inner-type lookup: pulls from the sibling enum's `Some` payload
/// if available (handles primitive cases like `OptionU32` → `u32` correctly,
/// where the prefix-strip would yield `U32`).
pub fn get_option_inner_type_ir(struct_def: &StructDef, ir: &CodegenIR) -> Option<String> {
    if let Some(e) = ir.find_enum(&struct_def.name) {
        if let Some(t) = get_option_inner_from_enum(e) {
            return Some(t);
        }
    }
    get_option_inner_type(struct_def)
}

// ============================================================================
// Type Wrapper Detection
// ============================================================================

/// Check if a type has a C++ wrapper class (not a typedef or callback)
///
/// Must agree with `should_skip_class`: a type with no wrapper emitted should
/// not claim it has one, otherwise call sites will reference an undeclared
/// wrapper name and the header won't compile.
pub fn type_has_wrapper(type_name: &str, ir: &CodegenIR) -> bool {
    if let Some(struct_def) = ir.find_struct(type_name) {
        // Skip categories that should_skip_class() also skips
        if matches!(
            struct_def.category,
            TypeCategory::CallbackTypedef
                | TypeCategory::GenericTemplate
                | TypeCategory::Recursive
                | TypeCategory::DestructorOrClone
        ) {
            return false;
        }
        // Skip types that render as simple type aliases
        if struct_def.fields.is_empty() && struct_def.traits.is_copy {
            return false;
        }
        return true;
    }
    // Option/Result tagged-union enums get a synthesized wrapper class in C++
    // codegen, so they count as having a wrapper for argument/return-type
    // substitution purposes (only if non-generic).
    if let Some(enum_def) = ir.find_enum(type_name) {
        if matches!(
            enum_def.category,
            TypeCategory::Option | TypeCategory::Result
        ) && enum_def.generic_params.is_empty()
        {
            return true;
        }
    }
    false
}

/// Check if a type needs the Proxy path for C++03 (non-copy wrapper class)
pub fn type_needs_proxy_for_cpp03(type_name: &str, ir: &CodegenIR) -> bool {
    if !type_has_wrapper(type_name, ir) {
        return false;
    }
    if let Some(struct_def) = ir.find_struct(type_name) {
        return !struct_def.traits.is_copy;
    }
    false
}

// ============================================================================
// Class/Struct Classification
// ============================================================================

/// Check if a struct should be skipped (callbacks, generic templates)
pub fn should_skip_class(struct_def: &StructDef) -> bool {
    matches!(
        struct_def.category,
        TypeCategory::CallbackTypedef |
        TypeCategory::GenericTemplate |
        TypeCategory::Recursive |
        // Note: VecRef types ARE included in C++ - they become simple wrapper classes
        // that expose ptr/len as std::span (C++20+) or raw pointers (earlier)
        TypeCategory::DestructorOrClone
    )
}

/// Check if a struct is a VecRef type (borrowed slice)
pub fn is_vecref_type(struct_def: &StructDef) -> bool {
    matches!(struct_def.category, TypeCategory::VecRef)
}

/// Check if a struct should render as a type alias instead of a class
pub fn renders_as_type_alias(struct_def: &StructDef) -> bool {
    // Option/Result wrappers always get a real class, even when their inner
    // payload is Copy and they look field-less to the rest of the pipeline.
    if matches!(
        struct_def.category,
        TypeCategory::Option | TypeCategory::Result
    ) {
        return false;
    }
    // Empty structs with Copy derive are just type aliases
    struct_def.fields.is_empty() && struct_def.traits.is_copy
}

/// Check if this is a simple enum (no data in variants)
pub fn is_simple_enum(enum_def: &EnumDef) -> bool {
    !enum_def.is_union
}

// ============================================================================
// Function Classification
// ============================================================================

/// Check if a function argument is a "self" parameter
pub fn is_self_arg(arg: &FunctionArg, class_name: &str) -> bool {
    // Self args have type_name == class_name and are the first arg
    arg.type_name == class_name || arg.name == "self" || arg.name == class_name.to_lowercase()
}

/// Check if a function has a self parameter
pub fn func_has_self(func: &FunctionDef) -> bool {
    func.args
        .first()
        .map(|a| is_self_arg(a, &func.class_name))
        .unwrap_or(false)
}

/// Check if a function is a "builder" method — an instance method whose
/// `method_name` is `with_*` or contains `_with_`. These are the methods
/// that, in C++23, get a `template<class Self> ... (this Self&& self, …)`
/// deducing-`this` form so they chain on l-values and r-values uniformly.
///
/// Static factories like `p_with_text` are excluded by the `func_has_self`
/// gate even though their name pattern matches.
pub fn is_builder_method(func: &FunctionDef) -> bool {
    if !func_has_self(func) {
        return false;
    }
    if !matches!(func.kind, FunctionKind::Method | FunctionKind::MethodMut) {
        return false;
    }
    func.method_name.starts_with("with_") || func.method_name.contains("_with_")
}

// ============================================================================
// Argument and Return Type Conversion
// ============================================================================

/// Convert a function argument to C++ type
/// If `substitute_callbacks` is true and the type is a callback wrapper,
/// use the raw C callback type instead (e.g., AzLayoutCallbackType instead of LayoutCallback)
pub fn arg_to_cpp_type_ex(
    arg: &FunctionArg,
    ir: &CodegenIR,
    config: &CodegenConfig,
    substitute_callbacks: bool,
) -> String {
    let base_type = &arg.type_name;

    // Check if this is a callback wrapper that should be substituted
    if substitute_callbacks {
        if let Some(callback_typedef) = get_callback_typedef_name(base_type, ir) {
            // Use the non-prefixed alias of the C fn-ptr typedef (e.g.
            // `LayoutCallbackType` = `AzLayoutCallbackType`). The alias is
            // emitted by generate_callback_typedef_aliases() right after the
            // forward declarations, so it is always in scope here. The alias
            // IS the C type, so user callbacks still have to be defined with
            // the raw C parameter structs (fn-ptr types must match exactly).
            return callback_typedef;
        }
    }

    if is_primitive(base_type) {
        let c_type = primitive_to_c(base_type);
        match arg.ref_kind {
            ArgRefKind::Ptr => format!("const {}*", c_type),
            ArgRefKind::PtrMut => format!("{}*", c_type),
            _ => c_type,
        }
    } else if type_has_wrapper(base_type, ir) {
        match arg.ref_kind {
            ArgRefKind::Ptr | ArgRefKind::Ref => format!("const {}&", base_type),
            ArgRefKind::PtrMut | ArgRefKind::RefMut => format!("{}&", base_type),
            ArgRefKind::Owned => base_type.clone(),
        }
    } else {
        let c_type = config.apply_prefix(base_type);
        match arg.ref_kind {
            ArgRefKind::Ptr => format!("const {}*", c_type),
            ArgRefKind::PtrMut => format!("{}*", c_type),
            _ => c_type,
        }
    }
}

/// Convert a function argument to C++ type (without callback substitution)
pub fn arg_to_cpp_type(arg: &FunctionArg, ir: &CodegenIR, config: &CodegenConfig) -> String {
    arg_to_cpp_type_ex(arg, ir, config, false)
}

/// Get C++ return type for a function
pub fn get_cpp_return_type(return_type: Option<&str>, ir: &CodegenIR) -> String {
    match return_type {
        None => "void".to_string(),
        Some(rt) => {
            let trimmed = rt.trim();

            // Handle pointer types: *const T, *mut T
            if trimmed.starts_with("*const ") {
                let inner = trimmed.strip_prefix("*const ").unwrap().trim();
                let c_inner = if is_primitive(inner) {
                    primitive_to_c(inner)
                } else {
                    format!("Az{}", inner)
                };
                return format!("const {}*", c_inner);
            }
            if trimmed.starts_with("*mut ") {
                let inner = trimmed.strip_prefix("*mut ").unwrap().trim();
                let c_inner = if is_primitive(inner) {
                    primitive_to_c(inner)
                } else {
                    format!("Az{}", inner)
                };
                return format!("{}*", c_inner);
            }

            if is_primitive(rt) {
                primitive_to_c(rt)
            } else if type_has_wrapper(rt, ir) {
                rt.to_string()
            } else {
                format!("Az{}", rt)
            }
        }
    }
}

/// Generate C++ function argument signature
pub fn generate_args_signature(
    args: &[FunctionArg],
    ir: &CodegenIR,
    config: &CodegenConfig,
    is_method: bool,
    class_name: &str,
) -> String {
    generate_args_signature_ex(args, ir, config, is_method, class_name, false)
}

/// Generate C++ argument signature with optional callback substitution
pub fn generate_args_signature_ex(
    args: &[FunctionArg],
    ir: &CodegenIR,
    config: &CodegenConfig,
    is_method: bool,
    class_name: &str,
    substitute_callbacks: bool,
) -> String {
    let mut result = Vec::new();

    for (i, arg) in args.iter().enumerate() {
        // Skip self parameter for methods
        if is_method && i == 0 && is_self_arg(arg, class_name) {
            continue;
        }

        let escaped_name = escape_cpp_keyword(&arg.name);
        let cpp_type = arg_to_cpp_type_ex(arg, ir, config, substitute_callbacks);
        result.push(format!("{} {}", cpp_type, escaped_name));
    }

    result.join(", ")
}

/// Generate C++ function call arguments
pub fn generate_call_args(
    args: &[FunctionArg],
    ir: &CodegenIR,
    config: &CodegenConfig,
    is_method: bool,
    class_name: &str,
) -> String {
    generate_call_args_ex(args, ir, config, is_method, class_name, false)
}

/// Returns true when the function has at least one `String` (wrapper) argument
/// and is eligible for a `std::string_view` sibling overload (C++17+).
///
/// Trait-generated functions (`_deepCopy`, `_partialEq`, etc.) are excluded —
/// they aren't user-facing and don't get callback substitution either.
pub fn func_takes_string_arg(func: &FunctionDef) -> bool {
    if !should_substitute_callbacks(func) {
        return false;
    }
    func.args.iter().enumerate().any(|(i, arg)| {
        if i == 0 && is_self_arg(arg, &func.class_name) {
            return false;
        }
        arg.type_name == "String" && matches!(arg.ref_kind, ArgRefKind::Owned)
    })
}

/// Generate the parameter list for a `std::string_view` overload — every
/// `String` (owned) argument becomes `std::string_view`; everything else keeps
/// its original C++ type.
pub fn generate_args_signature_sv_overload(
    args: &[FunctionArg],
    ir: &CodegenIR,
    config: &CodegenConfig,
    is_method: bool,
    class_name: &str,
    substitute_callbacks: bool,
) -> String {
    let mut result = Vec::new();
    for (i, arg) in args.iter().enumerate() {
        if is_method && i == 0 && is_self_arg(arg, class_name) {
            continue;
        }
        let escaped_name = escape_cpp_keyword(&arg.name);
        let cpp_type = if arg.type_name == "String" && matches!(arg.ref_kind, ArgRefKind::Owned) {
            "std::string_view".to_string()
        } else {
            arg_to_cpp_type_ex(arg, ir, config, substitute_callbacks)
        };
        result.push(format!("{} {}", cpp_type, escaped_name));
    }
    result.join(", ")
}

/// Generate the call-forwarding arguments for the `std::string_view` overload
/// body — wraps every string_view argument into a `String(sv)` to match the
/// underlying overload's `String` parameter; everything else keeps its
/// original call-arg shape.
pub fn generate_call_args_sv_overload(
    args: &[FunctionArg],
    ir: &CodegenIR,
    is_method: bool,
    class_name: &str,
) -> String {
    let mut result = Vec::new();
    for (i, arg) in args.iter().enumerate() {
        if is_method && i == 0 && is_self_arg(arg, class_name) {
            continue;
        }
        let escaped_name = escape_cpp_keyword(&arg.name);
        if arg.type_name == "String" && matches!(arg.ref_kind, ArgRefKind::Owned) {
            result.push(format!("String({})", escaped_name));
        } else if type_has_wrapper(&arg.type_name, ir) {
            let is_pointer = matches!(
                arg.ref_kind,
                ArgRefKind::Ptr | ArgRefKind::PtrMut | ArgRefKind::Ref | ArgRefKind::RefMut
            );
            if is_pointer {
                result.push(format!("{}", escaped_name));
            } else {
                result.push(format!("std::move({})", escaped_name));
            }
        } else {
            result.push(escaped_name);
        }
    }
    result.join(", ")
}

/// Generate C++ function call arguments with optional callback substitution
pub fn generate_call_args_ex(
    args: &[FunctionArg],
    ir: &CodegenIR,
    config: &CodegenConfig,
    is_method: bool,
    class_name: &str,
    substitute_callbacks: bool,
) -> String {
    let mut result = Vec::new();

    for (i, arg) in args.iter().enumerate() {
        // Skip self parameter for methods
        if is_method && i == 0 && is_self_arg(arg, class_name) {
            continue;
        }

        let escaped_name = escape_cpp_keyword(&arg.name);

        // A callback-wrapper argument is exposed as the raw `...Type` fn-ptr in
        // the C++ signature (ergonomics). How it is forwarded to the C ABI must
        // mirror exactly what lang_c emits for that argument:
        //   * `HOST_INVOKER_KINDS` wrappers get a raw fn-ptr C-ABI variant (the
        //     M2.5 pair pattern in lang_c) — pass the pointer straight through.
        //   * every other callback wrapper is taken by value as the wrapper
        //     struct, so rebuild it from the fn-ptr via `az_detail_wrap_cb`
        //     (the `FooCallbackType` typedef -> the `AzFooCallback` struct).
        if substitute_callbacks {
            if let Some(cb_typedef) = get_callback_typedef_name(&arg.type_name, ir) {
                if super::super::managed_host_invoker::is_callback_wrapper(&arg.type_name) {
                    result.push(escaped_name);
                } else {
                    let wrapper_struct = config
                        .apply_prefix(cb_typedef.strip_suffix("Type").unwrap_or(&cb_typedef));
                    result.push(format!("az_detail_wrap_cb<{}>({})", wrapper_struct, escaped_name));
                }
                continue;
            }
        }

        if type_has_wrapper(&arg.type_name, ir) {
            let is_pointer = matches!(
                arg.ref_kind,
                ArgRefKind::Ptr | ArgRefKind::PtrMut | ArgRefKind::Ref | ArgRefKind::RefMut
            );
            if is_pointer {
                result.push(format!("{}.ptr()", escaped_name));
            } else {
                result.push(format!("{}.release()", escaped_name));
            }
        } else {
            result.push(escaped_name);
        }
    }

    result.join(", ")
}

// ============================================================================
// Header Generation Helpers
// ============================================================================

/// Generate header comment block
pub fn generate_header_comment(standard: CppStandard) -> String {
    let mut code = String::new();

    code.push_str(
        "// =============================================================================\r\n",
    );
    code.push_str(&format!(
        "// Azul C++{} API Wrapper\r\n",
        standard.version_number()
    ));
    code.push_str(
        "// =============================================================================\r\n",
    );
    code.push_str("//\r\n");
    code.push_str(&format!(
        "// Compile with: g++ {} -o myapp myapp.cpp -lazul\r\n",
        standard.standard_flag()
    ));
    code.push_str("//\r\n");
    code.push_str("// This header provides C++ wrapper classes for the Azul C API.\r\n");
    code.push_str("// All classes use RAII for memory management.\r\n");
    code.push_str("//\r\n");

    code
}

/// Generate version-specific feature documentation
pub fn generate_feature_docs(standard: CppStandard) -> String {
    let mut code = String::new();

    if standard.has_string_view() {
        code.push_str("// C++17+ FEATURES:\r\n");
        code.push_str("//   - String supports std::string_view constructor and conversion\r\n");
        code.push_str(
            "//   - Option types support toStdOptional() and std::optional conversion\r\n",
        );
        code.push_str("//   - [[nodiscard]] attributes for static constructors\r\n");
        code.push_str("//\r\n");
    }
    if standard.has_span() {
        code.push_str("// C++20+ FEATURES:\r\n");
        code.push_str(
            "//   - Vec types support toSpan() and std::span conversion for zero-copy access\r\n",
        );
        code.push_str("//\r\n");
    }
    if standard.has_expected() {
        code.push_str("// C++23 FEATURES:\r\n");
        code.push_str(
            "//   - Result types support toStdExpected() and std::expected conversion\r\n",
        );
        code.push_str("//\r\n");
    }

    code
}

/// Generate C++03 Colvin-Gibbons documentation
pub fn generate_cpp03_move_docs() -> String {
    let mut code = String::new();

    code.push_str("// C++03 MOVE EMULATION (Colvin-Gibbons Trick)\r\n");
    code.push_str("// ============================================\r\n");
    code.push_str("//\r\n");
    code.push_str(
        "// C++03 lacks move semantics, which normally prevents returning non-copyable\r\n",
    );
    code.push_str(
        "// RAII objects by value. This header uses the \"Colvin-Gibbons trick\" to work\r\n",
    );
    code.push_str("// around this limitation.\r\n");
    code.push_str("//\r\n");
    code.push_str("// How it works:\r\n");
    code.push_str("// - Each non-copyable class has a nested 'Proxy' struct\r\n");
    code.push_str(
        "// - When returning by value, the object converts to Proxy (releasing ownership)\r\n",
    );
    code.push_str("// - The receiving object constructs from Proxy (acquiring ownership)\r\n");
    code.push_str("// - Direct copy construction transfers ownership (like std::auto_ptr)\r\n");
    code.push_str("//\r\n");
    code.push_str("// WARNING: These objects CANNOT be safely stored in C++03 STL containers!\r\n");
    code.push_str("//\r\n");

    code
}

/// Generate include guards
pub fn generate_include_guards_begin(standard: CppStandard) -> String {
    format!(
        "#ifndef AZUL_CPP{}_HPP\r\n#define AZUL_CPP{}_HPP\r\n\r\n",
        standard.version_number(),
        standard.version_number()
    )
}

/// Generate include guards end
pub fn generate_include_guards_end(standard: CppStandard) -> String {
    format!("#endif // AZUL_CPP{}_HPP\r\n", standard.version_number())
}

/// Generate standard includes based on C++ version
pub fn generate_includes(standard: CppStandard) -> String {
    let mut code = String::new();

    // C header
    code.push_str("extern \"C\" {\r\n");
    code.push_str("#include \"azul.h\"\r\n");
    code.push_str("}\r\n\r\n");

    // Standard includes
    if standard.has_move_semantics() {
        code.push_str("#include <cstdint>\r\n");
        code.push_str("#include <cstddef>\r\n");
        code.push_str("#include <cstring>\r\n");
        code.push_str("#include <new>\r\n"); // placement new in RefAny::create
        code.push_str("#include <utility>\r\n");
        code.push_str("#include <stdexcept>\r\n");
        code.push_str("#include <string>\r\n");
        code.push_str("#include <vector>\r\n");
    } else {
        code.push_str("#include <stdint.h>\r\n");
        code.push_str("#include <stddef.h>\r\n");
        code.push_str("#include <string.h>\r\n");
    }

    if standard.has_optional() {
        code.push_str("#include <optional>\r\n");
    }
    if standard.has_variant() {
        code.push_str("#include <variant>\r\n");
    }
    if standard.has_move_semantics() {
        code.push_str("#include <type_traits>\r\n");
    }
    if standard >= CppStandard::Cpp20 {
        code.push_str("#include <concepts>\r\n");
    }
    if standard.has_span() {
        code.push_str("#include <span>\r\n");
    }
    if standard.has_string_view() {
        code.push_str("#include <string_view>\r\n");
    }
    if standard.has_expected() {
        // <expected> (C++23 library) may be absent even when the -std flag is
        // accepted; the toStdExpected()/operator std::expected members are
        // guarded by the same macro, so only pull the header when present.
        code.push_str("#if defined(__has_include)\r\n#if __has_include(<expected>)\r\n#include <expected>\r\n#endif\r\n#endif\r\n");
    }
    if standard.has_std_function() {
        code.push_str("#include <functional>\r\n");
    }

    code.push_str("\r\n");

    // Rebuild a callback *wrapper struct* from the raw `...Type` function pointer.
    // Callback-taking methods expose the bare fn-ptr in their C++ signature for
    // ergonomics, but the C ABI functions take the wrapper struct (`{ cb, ... }`).
    // This sets `cb` and value-initialises any extra fields (e.g. `ctx`). Valid
    // C++03 through C++23 (templates + value-init + member assignment) — note a
    // braced temporary (`W{f}`) is not a valid rvalue in C++03, hence the helper.
    code.push_str(
        "template<class W, class F> inline W az_detail_wrap_cb(F f) { W w = W(); w.cb = f; return w; }\r\n\r\n",
    );

    code
}

/// Emit the `az_string_from_literal` helper. Used both by `AZ_REFLECT`
/// (C++03) and by the template-reflection helpers (C++11+), so it lives
/// outside `generate_reflect_macro` to be callable independently.
pub fn generate_az_string_from_literal_helper(standard: CppStandard) -> String {
    let mut code = String::new();
    code.push_str("// Helper to create AzString from string literal\r\n");
    if standard.has_move_semantics() {
        code.push_str("inline AzString az_string_from_literal(const char* s) {\r\n");
        code.push_str("    return AzString_copyFromBytes(reinterpret_cast<const uint8_t*>(s), 0, std::strlen(s));\r\n");
        code.push_str("}\r\n\r\n");
    } else {
        code.push_str("inline AzString az_string_from_literal(const char* s) {\r\n");
        code.push_str("    return AzString_copyFromBytes((const uint8_t*)s, 0, strlen(s));\r\n");
        code.push_str("}\r\n\r\n");
    }
    code
}

/// Generate AZ_REFLECT macro for RTTI support
pub fn generate_reflect_macro(standard: CppStandard) -> String {
    let mut code = String::new();
    code.push_str(&generate_az_string_from_literal_helper(standard));

    code.push_str(
        "// =============================================================================\r\n",
    );
    code.push_str("// AZ_REFLECT Macro - Runtime Type Information for user types\r\n");
    code.push_str(
        "// =============================================================================\r\n\r\n",
    );

    if standard.has_move_semantics() {
        code.push_str("#define AZ_REFLECT(structName) \\\r\n");
        code.push_str("    AZ_REFLECT_FULL(structName, 0, 0)\r\n\r\n");
        code.push_str("#define AZ_REFLECT_JSON(structName, toJsonFn, fromJsonFn) \\\r\n");
        code.push_str("    AZ_REFLECT_FULL(structName, reinterpret_cast<uintptr_t>(toJsonFn), reinterpret_cast<uintptr_t>(fromJsonFn))\r\n\r\n");
        code.push_str("#define AZ_REFLECT_FULL(structName, serializeFn, deserializeFn) \\\r\n");
        code.push_str("    namespace structName##_rtti { \\\r\n");
        code.push_str("        static const uint64_t type_id_storage = 0; \\\r\n");
        code.push_str("        inline uint64_t type_id() { return reinterpret_cast<uint64_t>(&type_id_storage); } \\\r\n");
        // Destroy-in-place only: the pointer belongs to the Rust-side alloc,
        // which Rust deallocates itself after invoking this destructor.
        code.push_str("        inline void destructor(void* ptr) { static_cast<structName*>(ptr)->~structName(); } \\\r\n");
        code.push_str("    } \\\r\n");
        code.push_str(
            "    static inline azul::RefAny structName##_upcast(structName model) { \\\r\n",
        );
        // AzRefAny_newC memcpys the bytes into a Rust-side allocation that
        // takes over ownership of the bits, so the temporary lives in stack
        // storage and is deliberately not destroyed here (destroy-in-place
        // happens once, via the destructor above, at last drop).
        code.push_str("        alignas(structName) unsigned char storage_[sizeof(structName)]; \\\r\n");
        code.push_str("        structName* tmp = ::new (static_cast<void*>(storage_)) structName(std::move(model)); \\\r\n");
        code.push_str("        AzGlVoidPtrConst ptr = { tmp, true }; \\\r\n");
        code.push_str("        AzString name = az_string_from_literal(#structName); \\\r\n");
        code.push_str("        return azul::RefAny(AzRefAny_newC(ptr, sizeof(structName), alignof(structName), \\\r\n");
        code.push_str("            structName##_rtti::type_id(), name, structName##_rtti::destructor, serializeFn, deserializeFn)); \\\r\n");
        code.push_str("    } \\\r\n");
        code.push_str("    static inline structName const* structName##_downcast_ref(azul::RefAny& data) { \\\r\n");
        code.push_str("        if (!AzRefAny_isType(&data.inner(), structName##_rtti::type_id())) return nullptr; \\\r\n");
        code.push_str(
            "        return static_cast<structName const*>(AzRefAny_getDataPtr(&data.inner())); \\\r\n",
        );
        code.push_str("    } \\\r\n");
        code.push_str(
            "    static inline structName* structName##_downcast_mut(azul::RefAny& data) { \\\r\n",
        );
        code.push_str("        if (!AzRefAny_isType(&data.inner(), structName##_rtti::type_id())) return nullptr; \\\r\n");
        code.push_str("        return static_cast<structName*>(const_cast<void*>(AzRefAny_getDataPtr(&data.inner()))); \\\r\n");
        code.push_str("    }\r\n\r\n");
    } else {
        code.push_str("#define AZ_REFLECT(structName) \\\r\n");
        code.push_str("    AZ_REFLECT_FULL(structName, 0, 0)\r\n\r\n");
        code.push_str("#define AZ_REFLECT_JSON(structName, toJsonFn, fromJsonFn) \\\r\n");
        code.push_str("    AZ_REFLECT_FULL(structName, (uintptr_t)(toJsonFn), (uintptr_t)(fromJsonFn))\r\n\r\n");
        code.push_str("#define AZ_REFLECT_FULL(structName, serializeFn, deserializeFn) \\\r\n");
        code.push_str("    static const uint64_t structName##_type_id_storage = 0; \\\r\n");
        code.push_str("    static uint64_t structName##_type_id() { return (uint64_t)(&structName##_type_id_storage); } \\\r\n");
        // Destroy-in-place only: the pointer belongs to the Rust-side alloc,
        // which Rust deallocates itself after invoking this destructor - a
        // `delete` here would free the same pointer twice, across allocators.
        code.push_str("    static void structName##_destructor(void* ptr) { ((structName*)ptr)->~structName(); } \\\r\n");
        // C++03 has no alignas/placement-new-into-stack idiom, so the value is
        // staged on the heap; AzRefAny_newC memcpys the bytes into a Rust-side
        // allocation that takes over ownership of the bits, after which the
        // staging block is returned raw via ::operator delete (matching the
        // ::operator new inside `new`, WITHOUT running ~structName - the bits
        // are destroyed exactly once, in place, at last drop).
        code.push_str("    static azul::RefAny structName##_upcast(structName model) { \\\r\n");
        code.push_str("        structName* heap = new structName(model); \\\r\n");
        code.push_str(
            "        AzGlVoidPtrConst ptr; ptr.ptr = heap; \\\r\n",
        );
        code.push_str("        AzString name = az_string_from_literal(#structName); \\\r\n");
        code.push_str("        azul::RefAny result(AzRefAny_newC(ptr, sizeof(structName), \\\r\n");
        code.push_str("            AZ_ALIGNOF(structName), structName##_type_id(), name, structName##_destructor, serializeFn, deserializeFn)); \\\r\n");
        code.push_str("        ::operator delete((void*)heap); \\\r\n");
        code.push_str("        return result; \\\r\n");
        code.push_str("    } \\\r\n");
        code.push_str(
            "    static structName const* structName##_downcast_ref(azul::RefAny& data) { \\\r\n",
        );
        code.push_str(
            "        if (!AzRefAny_isType(&data.inner(), structName##_type_id())) return 0; \\\r\n",
        );
        code.push_str("        return (structName const*)(AzRefAny_getDataPtr(&data.inner())); \\\r\n");
        code.push_str("    } \\\r\n");
        code.push_str(
            "    static structName* structName##_downcast_mut(azul::RefAny& data) { \\\r\n",
        );
        code.push_str(
            "        if (!AzRefAny_isType(&data.inner(), structName##_type_id())) return 0; \\\r\n",
        );
        code.push_str("        return (structName*)(AzRefAny_getDataPtr(&data.inner())); \\\r\n");
        code.push_str("    }\r\n\r\n");
    }

    code
}

/// Template-based reflection for C++11+ headers.
///
/// Emits `azul::upcast<T>`, `azul::downcast_ref<T>`, `azul::downcast_mut<T>`
/// function templates plus a `azul::type_id<T>()` helper. Per-type identity
/// is derived from the address of a template-instantiated `static const
/// uint64_t` - unique per `T`, with program-lifetime storage.
///
/// Must be emitted inside `namespace azul { ... }` after `class RefAny` is
/// fully declared (the templates inline-call `RefAny::inner()`).
///
/// C++14 picks up the `type_id_v<T>` variable template; older standards skip it.
/// Emit the namespace-level scaffolding the `RefAny` template member
/// functions need: per-type tag holder, type-erased destructor, and (in
/// C++20+) the `ReflectableModel` concept that constrains them.
///
/// The actual user-facing API — `RefAny::create<T>(T)`,
/// `RefAny::type_id<T>()`, `refany.downcast_ref<T>()`,
/// `refany.downcast_mut<T>()` — is emitted inside the `RefAny` class body
/// itself (via `generate_refany_template_members`).
pub fn generate_template_reflection(standard: CppStandard) -> String {
    if !standard.has_move_semantics() {
        return String::new();
    }

    let mut code = String::new();
    code.push_str("// =============================================================================\r\n");
    code.push_str("// Template-based reflection scaffolding for RefAny::create<T> et al.\r\n");
    code.push_str("// =============================================================================\r\n\r\n");

    code.push_str("namespace detail {\r\n");
    code.push_str("    // Per-type runtime tag, derived from the address of a template-instantiated\r\n");
    code.push_str("    // static. The address is unique per T and has program-lifetime storage.\r\n");
    code.push_str("    template<class T>\r\n");
    code.push_str("    struct type_id_holder { static const uint64_t value; };\r\n");
    code.push_str("    template<class T>\r\n");
    code.push_str("    const uint64_t type_id_holder<T>::value = 0;\r\n\r\n");
    code.push_str("    // Destroy-in-place ONLY: `ptr` points into a Rust-side allocation\r\n");
    code.push_str("    // (AzRefAny_newC memcpys the model into its own alloc and deallocates\r\n");
    code.push_str("    // that alloc itself right after invoking this destructor). A `delete`\r\n");
    code.push_str("    // here would free the same pointer twice, across two allocators.\r\n");
    code.push_str("    template<class T>\r\n");
    code.push_str("    inline void type_destructor(void* ptr) noexcept {\r\n");
    code.push_str("        static_cast<T*>(ptr)->~T();\r\n");
    code.push_str("    }\r\n");
    code.push_str("} // namespace detail\r\n\r\n");

    if standard >= CppStandard::Cpp20 {
        code.push_str("/// Structural concept: any object type T can be reflected as long as it is\r\n");
        code.push_str("/// destructible and isn't `RefAny` itself (wrapping a RefAny in a RefAny is\r\n");
        code.push_str("/// not what anyone wants). No per-class registration needed.\r\n");
        code.push_str("template<class T>\r\n");
        code.push_str("concept ReflectableModel = std::is_object_v<T> && std::is_destructible_v<T>\r\n");
        code.push_str("    && !std::is_same_v<T, RefAny>;\r\n\r\n");
    }

    code
}

/// Emit the user-facing template member functions of `RefAny`:
/// - `RefAny::create<T>(T model) -> RefAny` (static factory)
/// - `RefAny::type_id<T>() -> uint64_t` (static, runtime tag)
/// - `refany.downcast_ref<T>() -> const T*` (instance)
/// - `refany.downcast_mut<T>() -> T*` (instance)
///
/// Injected inside the `RefAny` class body so the API surface reads like
/// any other wrapper class - no namespace-level free functions.
pub fn generate_refany_template_members(standard: CppStandard) -> String {
    if !standard.has_move_semantics() {
        return String::new();
    }
    let mut code = String::new();
    let template_intro = if standard >= CppStandard::Cpp20 {
        "    template<ReflectableModel T>"
    } else {
        "    template<class T>"
    };

    code.push_str("\r\n    // Template-based reflection - C++11+ replacement for AZ_REFLECT.\r\n");

    code.push_str("    /// Per-type runtime tag - unique per T, stable across translation units.\r\n");
    code.push_str(&format!("{}\r\n", template_intro));
    code.push_str("    static uint64_t type_id() noexcept {\r\n");
    code.push_str("        return reinterpret_cast<uint64_t>(&detail::type_id_holder<T>::value);\r\n");
    code.push_str("    }\r\n\r\n");

    if standard >= CppStandard::Cpp14 {
        code.push_str("    /// Variable-template shorthand for `type_id<T>()` (C++14+).\r\n");
        code.push_str(&format!("{}\r\n", template_intro));
        if standard >= CppStandard::Cpp17 {
            // C++17 inline variable template - definition can live in-class.
            code.push_str("    static inline const uint64_t type_id_v = reinterpret_cast<uint64_t>(&detail::type_id_holder<T>::value);\r\n\r\n");
        } else {
            // C++14: declaration in-class, definition out-of-class (emitted
            // by `generate_refany_type_id_v_definition` after the class
            // body closes).
            code.push_str("    static const uint64_t type_id_v;\r\n\r\n");
        }
    }

    code.push_str("    /// Move T into a RefAny. The Rust-side equivalent of `RefAny::new(model)`.\r\n");
    code.push_str("    ///\r\n");
    code.push_str("    /// AzRefAny_newC memcpys the bytes into a Rust-side allocation that takes\r\n");
    code.push_str("    /// over ownership of the bits, so the temporary lives in stack storage\r\n");
    code.push_str("    /// and is deliberately NOT destroyed here: destroy-in-place happens\r\n");
    code.push_str("    /// exactly once, via detail::type_destructor<T> on the Rust-side buffer,\r\n");
    code.push_str("    /// when the last reference drops.\r\n");
    code.push_str(&format!("{}\r\n", template_intro));
    code.push_str("    static RefAny create(T model) {\r\n");
    code.push_str("        alignas(T) unsigned char storage_[sizeof(T)];\r\n");
    code.push_str("        T* tmp = ::new (static_cast<void*>(storage_)) T(std::move(model));\r\n");
    code.push_str("        AzGlVoidPtrConst ptr = { tmp, true };\r\n");
    code.push_str("        AzString name = az_string_from_literal(\"\");\r\n");
    code.push_str("        return RefAny(AzRefAny_newC(\r\n");
    code.push_str("            ptr,\r\n");
    code.push_str("            sizeof(T),\r\n");
    code.push_str("            alignof(T),\r\n");
    code.push_str("            RefAny::type_id<T>(),\r\n");
    code.push_str("            name,\r\n");
    code.push_str("            &detail::type_destructor<T>,\r\n");
    code.push_str("            0,\r\n");
    code.push_str("            0\r\n");
    code.push_str("        ));\r\n");
    code.push_str("    }\r\n\r\n");

    code.push_str("    /// Read-only borrow of the T inside this RefAny. nullptr on type mismatch.\r\n");
    code.push_str(&format!("{}\r\n", template_intro));
    code.push_str("    const T* downcast_ref() const noexcept {\r\n");
    code.push_str("        if (!AzRefAny_isType(&inner_, RefAny::type_id<T>())) return nullptr;\r\n");
    code.push_str("        return static_cast<const T*>(AzRefAny_getDataPtr(&inner_));\r\n");
    code.push_str("    }\r\n\r\n");

    code.push_str("    /// Mutable borrow of the T inside this RefAny. nullptr on type mismatch.\r\n");
    code.push_str(&format!("{}\r\n", template_intro));
    code.push_str("    T* downcast_mut() noexcept {\r\n");
    code.push_str("        if (!AzRefAny_isType(&inner_, RefAny::type_id<T>())) return nullptr;\r\n");
    code.push_str("        return static_cast<T*>(const_cast<void*>(AzRefAny_getDataPtr(&inner_)));\r\n");
    code.push_str("    }\r\n");

    code
}

/// Emit the out-of-class definition for `RefAny::type_id_v` on C++14 (where
/// inline variable templates aren't supported).
pub fn generate_refany_type_id_v_definition(standard: CppStandard) -> String {
    if standard != CppStandard::Cpp14 {
        return String::new();
    }
    let mut code = String::new();
    code.push_str("template<class T>\r\n");
    code.push_str("const uint64_t RefAny::type_id_v = reinterpret_cast<uint64_t>(&detail::type_id_holder<T>::value);\r\n\r\n");
    code
}

/// Emit free-function downcast helpers that operate directly on the C
/// struct `AzRefAny`, so callback bodies can call `azul::downcast_ref<T>(data)`
/// without first wrapping the parameter in the C++ `RefAny` class. The
/// callback parameter type is fixed to `AzRefAny` by the C function-pointer
/// typedef, so this is the cleanest in-body access pattern in C++.
///
/// Must be emitted INSIDE `namespace azul { ... }` after the
/// `detail::type_id_holder` template scaffolding (which it references) is in
/// scope.
pub fn generate_refany_freefn_downcasts(standard: CppStandard) -> String {
    if !standard.has_move_semantics() {
        return String::new();
    }
    let mut code = String::new();
    let template_intro = if standard >= CppStandard::Cpp20 {
        "template<ReflectableModel T>"
    } else {
        "template<class T>"
    };
    code.push_str("\r\n// Free-function downcast helpers. Operate directly on the C struct\r\n");
    code.push_str("// `AzRefAny` so callback bodies can write\r\n");
    code.push_str("//     auto* d = azul::downcast_ref<MyData>(data);\r\n");
    code.push_str("// The C function-pointer typedef fixes the parameter to `AzRefAny`,\r\n");
    code.push_str("// which has no methods of its own; these free templates fill the gap.\r\n");
    code.push_str("//\r\n");
    code.push_str("// NOTE: these helpers only READ - they do not manage ownership. The\r\n");
    code.push_str("// framework hands every callback an OWNED AzRefAny (a refcount+1\r\n");
    code.push_str("// clone), so adopt the parameter into azul::RefAny (whose destructor\r\n");
    code.push_str("// releases it) or call AzRefAny_delete(&data) before returning;\r\n");
    code.push_str("// otherwise one strong reference leaks per callback invocation.\r\n\r\n");
    code.push_str(&format!("{}\r\n", template_intro));
    code.push_str("inline const T* downcast_ref(const AzRefAny& data) noexcept {\r\n");
    code.push_str("    const uint64_t tag = reinterpret_cast<uint64_t>(&detail::type_id_holder<T>::value);\r\n");
    code.push_str("    if (!AzRefAny_isType(&data, tag)) return nullptr;\r\n");
    code.push_str("    return static_cast<const T*>(AzRefAny_getDataPtr(&data));\r\n");
    code.push_str("}\r\n\r\n");
    code.push_str(&format!("{}\r\n", template_intro));
    code.push_str("inline T* downcast_mut(AzRefAny& data) noexcept {\r\n");
    code.push_str("    const uint64_t tag = reinterpret_cast<uint64_t>(&detail::type_id_holder<T>::value);\r\n");
    code.push_str("    if (!AzRefAny_isType(&data, tag)) return nullptr;\r\n");
    code.push_str("    return static_cast<T*>(const_cast<void*>(AzRefAny_getDataPtr(&data)));\r\n");
    code.push_str("}\r\n");
    code
}

