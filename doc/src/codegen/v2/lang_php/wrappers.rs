//! Idiomatic PHP wrapper classes.
//!
//! For each Az-prefixed type with a corresponding `_delete` C function we
//! emit a `final class TypeName` inside `namespace Azul` that:
//!
//! - Stores the raw FFI cdata in a private property (`$ptr`).
//! - Implements `__destruct()` to call `Azul::lib()->Az<Type>_delete($ptr)`,
//!   forwarding the address via `FFI::addr(...)` so the C function gets a
//!   pointer to the boxed value.
//! - Surfaces every non-trait method on `TypeName` as an idiomatic instance
//!   or static method that delegates to the underlying FFI function.
//! - For tagged-union (data-bearing) enums, exposes per-variant predicates
//!   `isVariantName()` and per-variant payload extractors
//!   `payloadVariantName()` returning the FFI cdata of the variant payload.
//!
//! ## Skipped categories
//!
//! - `TypeCategory::Recursive`        — same reason as Python.
//! - `TypeCategory::VecRef`           — raw slice pointers, host-only.
//! - `TypeCategory::Boxed`            — internal heap wrappers.
//! - `TypeCategory::GenericTemplate`  — generic shells.
//! - `TypeCategory::DestructorOrClone`— internal callback typedefs.
//! - `TypeCategory::CallbackTypedef`  — function-pointer typedefs (the
//!   user-facing wrapper struct is emitted instead).
//! - Generic-parameterised types (those with non-empty `generic_params`).
//!
//! ## Naming
//!
//! Wrapper classes use the *unprefixed* IR name (`Azul\App`, not
//! `Azul\AzApp`). Method names on instances drop the leading
//! `<TypeName>_` C prefix (`$app->run(...)` instead of
//! `Azul::lib()->AzApp_run(...)`).

use super::super::ir::{
    CodegenIR, EnumDef, EnumVariantKind, FunctionDef, FunctionKind, StructDef, TypeCategory,
};

/// Generate the full wrapper section as a single PHP source string.
///
/// The output begins with a blank line so it inserts cleanly after the
/// trailing `}` of the static facade class.
pub fn generate_wrappers(ir: &CodegenIR) -> String {
    let mut out = String::new();
    out.push('\n');

    out.push_str("// ----------------------------------------------------------------------------\n");
    out.push_str("// Idiomatic wrapper classes (one per disposable struct / tagged union enum).\n");
    out.push_str("// ----------------------------------------------------------------------------\n");
    out.push('\n');

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
        emit_enum_wrapper(&mut out, ir, e);
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

fn has_delete_for(class: &str, ir: &CodegenIR) -> bool {
    ir.functions
        .iter()
        .any(|f| f.class_name == class && f.kind == FunctionKind::Delete)
}

// ============================================================================
// Struct wrappers
// ============================================================================

fn emit_struct_wrapper(out: &mut String, ir: &CodegenIR, s: &StructDef) {
    let class = sanitize_class_name(&s.name);
    let c_name = format!("Az{}", s.name);
    let funcs: Vec<&FunctionDef> = ir.functions_for_class(&s.name).collect();
    if funcs.is_empty() {
        // Without any functions there is nothing useful to wrap — skip.
        return;
    }
    let has_delete = has_delete_for(&s.name, ir);

    if !s.doc.is_empty() {
        out.push_str("/**\n");
        for d in &s.doc {
            out.push_str(&format!(" * {}\n", phpdoc_escape(d)));
        }
        out.push_str(" */\n");
    }

    out.push_str(&format!("final class {}\n", class));
    out.push_str("{\n");

    // Storage: the raw FFI cdata. Type-hint to FFI\CData so static
    // analysers understand the value, but we accept any cdata the user
    // hands us at construction.
    out.push_str("    /** @var \\FFI\\CData */\n");
    out.push_str("    private \\FFI\\CData $ptr;\n\n");

    // Internal raw constructor — wrapper classes consume an existing
    // FFI cdata. Public callers should use `create()` / `default()` /
    // other static factories below.
    out.push_str("    /**\n");
    out.push_str("     * Wrap an existing FFI cdata (takes ownership).\n");
    out.push_str("     *\n");
    out.push_str(&format!(
        "     * @param \\FFI\\CData $ptr a value of FFI type `{}`\n",
        c_name
    ));
    out.push_str("     */\n");
    out.push_str("    public function __construct(\\FFI\\CData $ptr)\n");
    out.push_str("    {\n");
    out.push_str("        $this->ptr = $ptr;\n");
    out.push_str("    }\n\n");

    // Raw accessor for power users / cross-class FFI plumbing.
    out.push_str("    /**\n");
    out.push_str("     * Return the underlying FFI cdata. Use with care.\n");
    out.push_str("     *\n");
    out.push_str("     * @return \\FFI\\CData\n");
    out.push_str("     */\n");
    out.push_str("    public function raw(): \\FFI\\CData\n");
    out.push_str("    {\n");
    out.push_str("        return $this->ptr;\n");
    out.push_str("    }\n\n");

    // Methods. We emit:
    //   - Instance methods for Method / MethodMut.
    //   - clone() for DeepCopy.
    //   - toString() for DebugToString (NOT __toString — we don't want
    //     PHP's casting magic to swallow native errors).
    //   - Static factories for Constructor / StaticMethod / Default.
    let mut emitted_any_instance = false;
    for f in &funcs {
        match f.kind {
            FunctionKind::Method | FunctionKind::MethodMut => {
                emit_instance_method(out, f, false);
                emitted_any_instance = true;
            }
            FunctionKind::DeepCopy => {
                emit_instance_method_alias(out, f, "clone");
                emitted_any_instance = true;
            }
            FunctionKind::DebugToString => {
                emit_instance_method_alias(out, f, "toString");
                emitted_any_instance = true;
            }
            FunctionKind::Constructor | FunctionKind::StaticMethod | FunctionKind::Default => {
                emit_static_factory(out, f, &class);
            }
            FunctionKind::Delete
            | FunctionKind::PartialEq
            | FunctionKind::PartialCmp
            | FunctionKind::Cmp
            | FunctionKind::Hash
            | FunctionKind::EnumVariantConstructor => {
                // SKIPPED: trait-only or enum-specific functions are not
                // surfaced as wrapper methods. Delete is wired through
                // __destruct() below; equality / ordering / hashing are
                // accessed via Azul::lib()->Az<Type>_<op> if needed.
            }
        }
    }
    if !emitted_any_instance {
        // PHP line comment to make the empty wrapper less surprising.
        out.push_str("    // (no instance methods)\n\n");
    }

    // Destructor — only emitted for types with an explicit `_delete`
    // function. Plain POD/Copy types have no native cleanup.
    if has_delete {
        out.push_str("    /**\n");
        out.push_str(&format!(
            "     * Free the underlying native resources by calling `{}_delete`.\n",
            c_name
        ));
        out.push_str("     */\n");
        out.push_str("    public function __destruct()\n");
        out.push_str("    {\n");
        out.push_str(&format!(
            "        Azul::lib()->{}_delete(\\FFI::addr($this->ptr));\n",
            c_name
        ));
        out.push_str("    }\n");
    } else {
        out.push_str("    // SKIPPED: no _delete C function — relying on PHP GC for the cdata.\n");
    }

    out.push_str("}\n\n");
}

// ============================================================================
// Tagged-union enum wrappers
// ============================================================================

fn emit_enum_wrapper(out: &mut String, ir: &CodegenIR, e: &EnumDef) {
    let class = sanitize_class_name(&e.name);
    let c_name = format!("Az{}", e.name);
    let funcs: Vec<&FunctionDef> = ir.functions_for_class(&e.name).collect();
    let has_delete = has_delete_for(&e.name, ir);

    if !e.doc.is_empty() {
        out.push_str("/**\n");
        for d in &e.doc {
            out.push_str(&format!(" * {}\n", phpdoc_escape(d)));
        }
        out.push_str(" */\n");
    }

    out.push_str(&format!("final class {}\n", class));
    out.push_str("{\n");

    // Storage.
    out.push_str("    /** @var \\FFI\\CData */\n");
    out.push_str("    private \\FFI\\CData $ptr;\n\n");

    out.push_str("    /**\n");
    out.push_str(&format!(
        "     * Wrap an existing FFI cdata (a `{}` value or pointer).\n",
        c_name
    ));
    out.push_str("     */\n");
    out.push_str("    public function __construct(\\FFI\\CData $ptr)\n");
    out.push_str("    {\n");
    out.push_str("        $this->ptr = $ptr;\n");
    out.push_str("    }\n\n");

    out.push_str("    /**\n");
    out.push_str("     * Return the underlying FFI cdata. Use with care.\n");
    out.push_str("     *\n");
    out.push_str("     * @return \\FFI\\CData\n");
    out.push_str("     */\n");
    out.push_str("    public function raw(): \\FFI\\CData\n");
    out.push_str("    {\n");
    out.push_str("        return $this->ptr;\n");
    out.push_str("    }\n\n");

    // For union enums (data-bearing), surface a `tag()` accessor and
    // per-variant `is<Variant>()` / `payload<Variant>()` helpers. The
    // C-ABI emits the discriminator under a `tag` field on each variant
    // payload struct, with the canonical convention `variant.tag ==
    // Az<Enum>_Tag_<Variant>`. Every variant struct in the FFI union
    // carries its own copy of the same tag, so reading from the first
    // variant's `tag` is always valid.
    if e.is_union {
        // SKIPPED: PHP's FFI cdata does not let us reach the
        // discriminator field by *name* without knowing which arm of the
        // union is currently active. We instead read it through the
        // payload of the first variant, which is always layout-compatible
        // because every variant struct begins with the same `tag` field.
        if let Some(first_variant) = e.variants.first() {
            let first_field = sanitize_php_identifier(&first_variant.name);
            out.push_str("    /**\n");
            out.push_str("     * Return the variant discriminator tag value (as an int).\n");
            out.push_str("     *\n");
            out.push_str("     * @return int one of the `Az<Enum>_Tag_*` constants from the cdef.\n");
            out.push_str("     */\n");
            out.push_str("    public function tag(): int\n");
            out.push_str("    {\n");
            out.push_str(&format!(
                "        return $this->ptr->{}->tag;\n",
                first_field
            ));
            out.push_str("    }\n\n");
        }

        for v in &e.variants {
            let php_field = sanitize_php_identifier(&v.name);
            let pred = format!("is{}", v.name);
            let pay = format!("payload{}", v.name);
            let tag_const = format!("{}_Tag_{}", c_name, v.name);

            out.push_str("    /**\n");
            out.push_str(&format!(
                "     * True if this {} value carries the {} variant.\n",
                e.name, v.name
            ));
            out.push_str("     */\n");
            out.push_str(&format!("    public function {}(): bool\n", pred));
            out.push_str("    {\n");
            out.push_str(&format!(
                "        return $this->tag() === Azul::lib()->{};\n",
                tag_const
            ));
            out.push_str("    }\n\n");

            match &v.kind {
                EnumVariantKind::Unit => {
                    out.push_str(&format!(
                        "    // SKIPPED: payload{}() omitted for unit variant {}.\n\n",
                        v.name, v.name
                    ));
                }
                EnumVariantKind::Tuple(_) | EnumVariantKind::Struct(_) => {
                    out.push_str("    /**\n");
                    out.push_str(&format!(
                        "     * Return the FFI cdata payload for the {} variant.\n",
                        v.name
                    ));
                    out.push_str("     *\n");
                    out.push_str(&format!(
                        "     * The caller must ensure {}() is true; otherwise the returned\n",
                        pred
                    ));
                    out.push_str("     * value reads garbage memory because of the union layout.\n");
                    out.push_str("     *\n");
                    out.push_str("     * @return mixed FFI cdata of the variant payload struct\n");
                    out.push_str("     */\n");
                    out.push_str(&format!("    public function {}()\n", pay));
                    out.push_str("    {\n");
                    out.push_str(&format!(
                        "        return $this->ptr->{};\n",
                        php_field
                    ));
                    out.push_str("    }\n\n");
                }
            }
        }
    }

    // Methods + static factories (same shape as struct wrappers).
    let mut emitted_any_instance = false;
    for f in &funcs {
        match f.kind {
            FunctionKind::Method | FunctionKind::MethodMut => {
                emit_instance_method(out, f, true);
                emitted_any_instance = true;
            }
            FunctionKind::DeepCopy => {
                emit_instance_method_alias(out, f, "clone");
                emitted_any_instance = true;
            }
            FunctionKind::DebugToString => {
                emit_instance_method_alias(out, f, "toString");
                emitted_any_instance = true;
            }
            FunctionKind::EnumVariantConstructor
            | FunctionKind::Constructor
            | FunctionKind::StaticMethod
            | FunctionKind::Default => {
                emit_static_factory(out, f, &class);
            }
            FunctionKind::Delete
            | FunctionKind::PartialEq
            | FunctionKind::PartialCmp
            | FunctionKind::Cmp
            | FunctionKind::Hash => {
                // SKIPPED: trait-only.
            }
        }
    }
    if !emitted_any_instance && !e.is_union {
        out.push_str("    // (no instance methods)\n\n");
    }

    if has_delete {
        out.push_str("    /**\n");
        out.push_str(&format!(
            "     * Free the underlying native resources by calling `{}_delete`.\n",
            c_name
        ));
        out.push_str("     */\n");
        out.push_str("    public function __destruct()\n");
        out.push_str("    {\n");
        out.push_str(&format!(
            "        Azul::lib()->{}_delete(\\FFI::addr($this->ptr));\n",
            c_name
        ));
        out.push_str("    }\n");
    } else {
        out.push_str("    // SKIPPED: no _delete C function — relying on PHP GC for the cdata.\n");
    }

    out.push_str("}\n\n");
}

// ============================================================================
// Method / factory emission helpers
// ============================================================================

/// Emit an instance method. `_takes_union_ptr` is currently unused but
/// kept as a hook for future enum-specific behaviour (the union wrapper
/// could decide to forward `$this->ptr` directly rather than `FFI::addr`).
fn emit_instance_method(out: &mut String, f: &FunctionDef, _takes_union_ptr: bool) {
    let php_name = sanitize_php_identifier(&f.method_name);
    let user_args = user_args(f);

    let params = render_php_params(&user_args);
    let user_call_args = render_call_args(&user_args);

    out.push_str("    /**\n");
    if !f.doc.is_empty() {
        for d in &f.doc {
            out.push_str(&format!("     * {}\n", phpdoc_escape(d)));
        }
        out.push_str("     *\n");
    }
    out.push_str(&format!(
        "     * Wraps `Azul::lib()->{}` with the receiver bound to `$this`.\n",
        f.c_name
    ));
    out.push_str("     */\n");
    out.push_str(&format!(
        "    public function {}({})\n",
        php_name, params
    ));
    out.push_str("    {\n");

    // The C function takes the receiver as `<ClassName>*` (a pointer
    // to the FFI struct). We forward via FFI::addr so the C side gets
    // a stable address into our cdata.
    let mut call = format!("Azul::lib()->{}(\\FFI::addr($this->ptr)", f.c_name);
    if !user_call_args.is_empty() {
        call.push_str(", ");
        call.push_str(&user_call_args);
    }
    call.push(')');

    if f.return_type.is_none() {
        out.push_str(&format!("        {};\n", call));
    } else {
        out.push_str(&format!("        return {};\n", call));
    }
    out.push_str("    }\n\n");
}

/// Variant of `emit_instance_method` that uses an idiomatic PHP method
/// name (e.g. `clone`, `toString`) regardless of the C method name.
fn emit_instance_method_alias(out: &mut String, f: &FunctionDef, php_name: &str) {
    let user_args = user_args(f);
    let params = render_php_params(&user_args);
    let user_call_args = render_call_args(&user_args);

    out.push_str("    /**\n");
    out.push_str(&format!(
        "     * Idiomatic alias dispatching to `Azul::lib()->{}`.\n",
        f.c_name
    ));
    out.push_str("     */\n");
    out.push_str(&format!(
        "    public function {}({})\n",
        php_name, params
    ));
    out.push_str("    {\n");

    let mut call = format!("Azul::lib()->{}(\\FFI::addr($this->ptr)", f.c_name);
    if !user_call_args.is_empty() {
        call.push_str(", ");
        call.push_str(&user_call_args);
    }
    call.push(')');

    if f.return_type.is_none() {
        out.push_str(&format!("        {};\n", call));
    } else {
        out.push_str(&format!("        return {};\n", call));
    }
    out.push_str("    }\n\n");
}

/// Emit a `public static` factory. Constructors and StaticMethods that
/// return the same type are wrapped back into the wrapper class via
/// `new self(...)`; everything else returns the raw FFI cdata for the
/// caller to handle.
fn emit_static_factory(out: &mut String, f: &FunctionDef, class_name: &str) {
    let php_name = sanitize_php_identifier(&f.method_name);
    let user_args = user_args(f);
    let params = render_php_params(&user_args);
    let user_call_args = render_call_args(&user_args);

    let returns_self = f
        .return_type
        .as_deref()
        .map(|r| r.trim() == f.class_name)
        .unwrap_or(false);

    out.push_str("    /**\n");
    if !f.doc.is_empty() {
        for d in &f.doc {
            out.push_str(&format!("     * {}\n", phpdoc_escape(d)));
        }
        out.push_str("     *\n");
    }
    out.push_str(&format!(
        "     * Wraps `Azul::lib()->{}`.\n",
        f.c_name
    ));
    if returns_self {
        out.push_str(&format!(
            "     *\n     * @return self instance wrapping the returned FFI cdata.\n"
        ));
    }
    out.push_str("     */\n");
    let return_hint = if returns_self { ": self" } else { "" };
    out.push_str(&format!(
        "    public static function {}({}){}\n",
        php_name, params, return_hint
    ));
    out.push_str("    {\n");

    let call = format!("Azul::lib()->{}({})", f.c_name, user_call_args);
    if returns_self {
        out.push_str(&format!("        return new self({});\n", call));
    } else if f.return_type.is_none() {
        out.push_str(&format!("        {};\n", call));
    } else {
        out.push_str(&format!("        return {};\n", call));
    }
    out.push_str("    }\n\n");

    let _ = class_name;
}

// ============================================================================
// Argument helpers
// ============================================================================

/// Filter the implicit `self` / lower-class-name receiver out of a
/// function's arguments — the receiver is supplied by `$this` for
/// instance methods, and is absent entirely for static factories.
fn user_args<'a>(f: &'a FunctionDef) -> Vec<&'a super::super::ir::FunctionArg> {
    let class_lower = f.class_name.to_lowercase();
    f.args
        .iter()
        .filter(|a| a.name != "self" && a.name != class_lower)
        .collect()
}

/// Render `$name1, $name2, ...` for PHP method parameter lists. We do
/// not emit type hints because FFI cdata values cannot be reliably
/// type-hinted at the language level (every value is `\FFI\CData`,
/// which is too coarse to be useful).
fn render_php_params(args: &[&super::super::ir::FunctionArg]) -> String {
    args.iter()
        .map(|a| format!("${}", sanitize_php_identifier(&a.name)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Render the call-site arguments (same shape as parameters: `$name`
/// each).
fn render_call_args(args: &[&super::super::ir::FunctionArg]) -> String {
    args.iter()
        .map(|a| format!("${}", sanitize_php_identifier(&a.name)))
        .collect::<Vec<_>>()
        .join(", ")
}

// ============================================================================
// Identifier helpers
// ============================================================================

/// Pick a safe PHP class name. We use the IR type name verbatim (no
/// `Az` prefix) because the wrapper lives inside `namespace Azul`. PHP
/// class names cannot collide with reserved words at the *unqualified*
/// position because they are always invoked through the namespace.
fn sanitize_class_name(raw: &str) -> String {
    raw.to_string()
}

/// Sanitize an identifier for use as a PHP method/property/parameter
/// name. PHP reserves a small set of names that cannot appear bare.
fn sanitize_php_identifier(name: &str) -> String {
    match name {
        // Reserved PHP keywords that would otherwise collide as method
        // names. We append a trailing `_` rather than a leading one so
        // we do not produce names starting with `_` (which has its own
        // soft-private connotation in PHP code style).
        "class" | "function" | "list" | "new" | "echo" | "print" | "default" | "switch"
        | "case" | "break" | "continue" | "for" | "foreach" | "while" | "do" | "if" | "else"
        | "elseif" | "and" | "or" | "xor" | "namespace" | "use" | "trait" | "interface"
        | "abstract" | "final" | "private" | "public" | "protected" | "static" | "var"
        | "const" | "global" | "try" | "catch" | "finally" | "throw" | "return" | "yield"
        | "include" | "require" | "include_once" | "require_once" | "match" | "fn"
        | "array" | "callable" | "bool" | "int" | "float" | "string" | "void" | "iterable"
        | "object" | "mixed" | "never" | "self" | "parent" | "true" | "false" | "null" => {
            format!("{}_", name)
        }
        _ => name.to_string(),
    }
}

/// Escape PHPDoc-meaningful characters in a free-form doc line.
/// Currently only `*/` (which would close the block early) needs care.
fn phpdoc_escape(s: &str) -> String {
    s.replace("*/", "* /")
}
