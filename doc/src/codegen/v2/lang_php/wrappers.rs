//! Idiomatic PHP wrapper classes.
//!
//! For each Az-prefixed type with a corresponding `_delete` C function we
//! emit a `final class TypeName` inside `namespace Azul` that:
//!
//! - Stores the raw FFI cdata in a private property (`$ptr`).
//! - Implements `__destruct()` to call `Azul::lib()->Az<Type>_delete($ptr)`, forwarding the address
//!   via `FFI::addr(...)` so the C function gets a pointer to the boxed value.
//! - Surfaces every non-trait method on `TypeName` as an idiomatic instance or static method that
//!   delegates to the underlying FFI function.
//! - For tagged-union (data-bearing) enums, exposes per-variant predicates `isVariantName()` and
//!   per-variant payload extractors `payloadVariantName()` returning the FFI cdata of the variant
//!   payload.
//!
//! ## Skipped categories
//!
//! - `TypeCategory::Recursive`        — same reason as Python.
//! - `TypeCategory::VecRef`           — raw slice pointers, host-only.
//! - `TypeCategory::Boxed`            — internal heap wrappers.
//! - `TypeCategory::GenericTemplate`  — generic shells.
//! - `TypeCategory::DestructorOrClone`— internal callback typedefs.
//! - `TypeCategory::CallbackTypedef`  — function-pointer typedefs (the user-facing wrapper struct
//!   is emitted instead).
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

    out.push_str(
        "// ----------------------------------------------------------------------------\n",
    );
    out.push_str("// Idiomatic wrapper classes (one per disposable struct / tagged union enum).\n");
    out.push_str(
        "// ----------------------------------------------------------------------------\n",
    );
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
    !matches!(s.category, TypeCategory::Recursive
        | TypeCategory::VecRef
        | TypeCategory::Boxed
        | TypeCategory::GenericTemplate
        | TypeCategory::DestructorOrClone
        | TypeCategory::CallbackTypedef)
}

fn should_emit_enum(e: &EnumDef) -> bool {
    if !e.generic_params.is_empty() {
        return false;
    }
    !matches!(e.category, TypeCategory::Recursive
        | TypeCategory::VecRef
        | TypeCategory::Boxed
        | TypeCategory::GenericTemplate
        | TypeCategory::DestructorOrClone
        | TypeCategory::CallbackTypedef)
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

    // Storage, constructor, `raw()` and `intoRaw()` (shared with the
    // union wrappers).
    emit_storage(out, ir, &s.name, &c_name);

    // Methods. We emit:
    //   - Instance methods for Method / MethodMut.
    //   - clone() for DeepCopy.
    //   - toString() for DebugToString (NOT __toString — we don't want PHP's casting magic to
    //     swallow native errors).
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
    // Typed field accessors `get_<field>()` / `set_<field>($value)`. An
    // api.json method of the same name wins.
    let mut taken: std::collections::BTreeSet<String> = funcs
        .iter()
        .map(|f| sanitize_php_identifier(&f.method_name).to_lowercase())
        .collect();
    if emit_field_accessors(out, ir, s, &mut taken) > 0 {
        emitted_any_instance = true;
    }
    if !emitted_any_instance {
        // PHP line comment to make the empty wrapper less surprising.
        out.push_str("    // (no instance methods)\n\n");
    }

    // Destructor — only emitted for types with an explicit `_delete`
    // function. Plain POD/Copy types have no native cleanup.
    emit_destructor(out, has_delete, &c_name);

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

    // Storage, constructor, `raw()` and `intoRaw()`.
    emit_storage(out, ir, &e.name, &c_name);

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
            out.push_str(
                "     * @return int one of the `Az<Enum>_Tag_*` constants from the cdef.\n",
            );
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
                    out.push_str(
                        "     * value reads garbage memory because of the union layout.\n",
                    );
                    out.push_str("     *\n");
                    out.push_str("     * @return mixed FFI cdata of the variant payload struct\n");
                    out.push_str("     */\n");
                    out.push_str(&format!("    public function {}()\n", pay));
                    out.push_str("    {\n");
                    out.push_str(&format!("        return $this->ptr->{};\n", php_field));
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

    emit_destructor(out, has_delete, &c_name);

    out.push_str("}\n\n");
}

// ============================================================================
// Ownership: storage, moves, destruction
// ============================================================================

/// The wrapper's storage, constructor, `raw()` and `intoRaw()`.
///
/// A wrapper either OWNS its cdata (`$owner === null`; `__destruct` frees
/// it) or is a VIEW of a field of another wrapper (`$owner` keeps that
/// wrapper - and so the memory - alive; nothing is freed). `$ptr` is null
/// once the value was moved out (a by-value call took it, or `intoRaw()`).
fn emit_storage(out: &mut String, ir: &CodegenIR, type_name: &str, c_name: &str) {
    out.push_str("    /** @var ?\\FFI\\CData the value; null once it was moved out */\n");
    out.push_str("    private ?\\FFI\\CData $ptr;\n\n");
    out.push_str("    /** @var ?object the wrapper whose field this is a view of (never freed here) */\n");
    out.push_str("    private ?object $owner;\n\n");

    out.push_str("    /**\n");
    out.push_str("     * Wrap an existing FFI cdata. Without `$owner` the wrapper takes ownership;\n");
    out.push_str("     * with it, the cdata is a field of `$owner` and stays owned by it.\n");
    out.push_str("     *\n");
    out.push_str(&format!(
        "     * @param \\FFI\\CData $ptr a value of FFI type `{}`\n",
        c_name
    ));
    out.push_str("     */\n");
    out.push_str("    public function __construct(\\FFI\\CData $ptr, ?object $owner = null)\n");
    out.push_str("    {\n");
    out.push_str("        $this->ptr = $ptr;\n");
    out.push_str("        $this->owner = $owner;\n");
    out.push_str("    }\n\n");

    out.push_str("    /**\n");
    out.push_str("     * Return the underlying FFI cdata (still owned by this wrapper). Use with care.\n");
    out.push_str("     *\n");
    out.push_str("     * @return ?\\FFI\\CData\n");
    out.push_str("     */\n");
    out.push_str("    public function raw(): ?\\FFI\\CData\n");
    out.push_str("    {\n");
    out.push_str("        return $this->ptr;\n");
    out.push_str("    }\n\n");

    let delete = ir
        .functions
        .iter()
        .find(|f| f.class_name == type_name && f.kind == FunctionKind::Delete);
    let clone = ir
        .functions
        .iter()
        .find(|f| f.class_name == type_name && f.kind == FunctionKind::DeepCopy);
    out.push_str("    /**\n");
    out.push_str("     * Move the value out, to hand it to something that takes it by value (a\n");
    out.push_str("     * field setter): this wrapper no longer frees it. A field view hands out a\n");
    out.push_str("     * copy instead, so its owner keeps the field.\n");
    out.push_str("     */\n");
    out.push_str("    public function intoRaw(): \\FFI\\CData\n");
    out.push_str("    {\n");
    out.push_str("        if ($this->ptr === null) {\n");
    out.push_str(&format!(
        "            throw new \\LogicException('{}: this value was moved');\n",
        type_name
    ));
    out.push_str("        }\n");
    out.push_str("        if ($this->owner !== null) {\n");
    match (delete, clone) {
        (_, Some(c)) => out.push_str(&format!(
            "            return Azul::lib()->{}(\\FFI::addr($this->ptr));\n",
            c.c_name
        )),
        (None, None) => {
            out.push_str(&format!("            $c = Azul::lib()->new('{}');\n", c_name));
            out.push_str("            \\FFI::memcpy($c, $this->ptr, \\FFI::sizeof($c));\n");
            out.push_str("            return $c;\n");
        }
        (Some(_), None) => out.push_str(&format!(
            "            throw new \\LogicException('{}: a field view has no deep copy; pass a fresh value');\n",
            type_name
        )),
    }
    out.push_str("        }\n");
    out.push_str("        $p = $this->ptr;\n");
    if delete.is_some() {
        out.push_str("        $this->ptr = null;\n");
    }
    out.push_str("        return $p;\n");
    out.push_str("    }\n\n");
}

/// `__destruct` for a type with a `_delete` export: frees an OWNED, not
/// moved-out value - never a field view, never a moved-out wrapper.
fn emit_destructor(out: &mut String, has_delete: bool, c_name: &str) {
    if has_delete {
        out.push_str("    /**\n");
        out.push_str(&format!(
            "     * Free the underlying native resources by calling `{}_delete` (not for a\n",
            c_name
        ));
        out.push_str("     * field view, whose owner frees it, nor for a moved-out value).\n");
        out.push_str("     */\n");
        out.push_str("    public function __destruct()\n");
        out.push_str("    {\n");
        out.push_str("        if ($this->ptr === null || $this->owner !== null) {\n");
        out.push_str("            return;\n");
        out.push_str("        }\n");
        out.push_str(&format!(
            "        Azul::lib()->{}_delete(\\FFI::addr($this->ptr));\n",
            c_name
        ));
        out.push_str("    }\n");
    } else {
        out.push_str("    // SKIPPED: no _delete C function — relying on PHP GC for the cdata.\n");
    }
}

// ============================================================================
// Field access
// ============================================================================

/// The wrapper class a field of IR type `name` is viewed through, if one is
/// emitted (a struct with exports of its own, or a tagged union).
fn php_view_class(name: &str, ir: &CodegenIR) -> Option<String> {
    if let Some(s) = ir.find_struct(name) {
        return (should_emit_struct(s) && ir.functions_for_class(name).next().is_some())
            .then(|| sanitize_class_name(name));
    }
    if let Some(e) = ir.find_enum(name) {
        return (should_emit_enum(e) && e.is_union).then(|| sanitize_class_name(name));
    }
    None
}

/// `get_<field>()` / `set_<field>($value)` for every public field (the
/// shared contract in `field_access`):
///
/// - scalars (`bool`, numbers, unit enums as `int`) read and write in place;
/// - the String class reads as a PHP string (decoded, the field is not
///   consumed); its setter takes a PHP string (copied into a fresh
///   AzString) or an `AzString` wrapper (moved in);
/// - a struct / union field reads as a LIVE VIEW - a wrapper over the
///   parent's memory that is never freed and keeps the parent alive - so
///   `$opts->get_window_state()->set_title('Hello')` changes `$opts`.
///   `->clone()` copies a view out. A field type without a wrapper class
///   reads as the raw FFI cdata view;
/// - every setter of a heap-owning field releases the old value
///   (`Az<T>_delete`) and then moves the new one in (`intoRaw()`: a wrapper
///   is consumed, a view deep-copied).
///
/// Returns the number of methods emitted.
fn emit_field_accessors(
    out: &mut String,
    ir: &CodegenIR,
    s: &StructDef,
    taken: &mut std::collections::BTreeSet<String>,
) -> usize {
    use super::super::field_access::{accessible_fields, FieldShape};
    let config = super::super::config::CodegenConfig::c_header();
    let mut n = 0;
    for (f, shape) in accessible_fields(s, ir, &config) {
        // The cdef spells fields the way azul.h does.
        let key = super::super::lang_c::escape_cpp_keyword_for_c(&f.name);
        let field = format!("$this->ptr->{}", key);
        let doc = f
            .doc
            .as_deref()
            .and_then(|d| d.lines().map(str::trim).find(|l| !l.is_empty()))
            .map(phpdoc_escape);

        // Getter.
        let get = format!("get_{}", f.name);
        let getter: (String, String) = match &shape {
            FieldShape::Prim { is_bool: true, .. } => ("bool".into(), format!("return {};", field)),
            FieldShape::Prim { ty, .. } if ty.starts_with('f') || ty.contains("float") || ty.contains("double") => {
                ("float".into(), format!("return {};", field))
            }
            FieldShape::Prim { .. } | FieldShape::UnitEnum { .. } => {
                ("int".into(), format!("return {};", field))
            }
            FieldShape::Str { .. } => ("string".into(), format!("return Azul::readString({});", field)),
            FieldShape::Value { name, .. } => match php_view_class(name, ir) {
                Some(cls) => (cls.clone(), format!("return new {}({}, $this);", cls, field)),
                None => ("\\FFI\\CData".into(), format!("return {};", field)),
            },
        };
        if taken.insert(get.to_lowercase()) {
            out.push_str("    /**\n");
            match &shape {
                FieldShape::Value { .. } => out.push_str(&format!(
                    "     * A live view of the `{}` field: writes through it reach this value.\n",
                    f.name
                )),
                _ => out.push_str(&format!("     * The `{}` field.\n", f.name)),
            }
            if let Some(d) = &doc {
                out.push_str(&format!("     *\n     * {}\n", d));
            }
            out.push_str("     */\n");
            out.push_str(&format!("    public function {}(): {}\n", get, getter.0));
            out.push_str("    {\n");
            out.push_str(&format!("        {}\n", getter.1));
            out.push_str("    }\n\n");
            n += 1;
        }

        // Setter.
        let set = format!("set_{}", f.name);
        if !taken.insert(set.to_lowercase()) {
            continue;
        }
        let mut body: Vec<String> = Vec::new();
        match &shape {
            FieldShape::Prim { .. } | FieldShape::UnitEnum { .. } => {
                body.push(format!("{} = $value;", field));
            }
            FieldShape::Str { name, delete } => {
                let cls = sanitize_class_name(name);
                body.push(format!(
                    "$new = \\is_string($value) ? Azul::str($value) : ($value instanceof {} ? $value->intoRaw() : $value);",
                    cls
                ));
                body.push(format!("Azul::lib()->{}(\\FFI::addr({}));", delete.c_name, field));
                body.push(format!("{} = $new;", field));
            }
            FieldShape::Value { name, delete, .. } => {
                match php_view_class(name, ir) {
                    Some(cls) => body.push(format!(
                        "$new = $value instanceof {} ? $value->intoRaw() : $value;",
                        cls
                    )),
                    None => body.push("$new = $value;".to_string()),
                }
                if let Some(d) = delete {
                    body.push(format!("Azul::lib()->{}(\\FFI::addr({}));", d.c_name, field));
                }
                body.push(format!("{} = $new;", field));
            }
        }
        out.push_str("    /**\n");
        match &shape {
            FieldShape::Prim { .. } | FieldShape::UnitEnum { .. } => {
                out.push_str(&format!("     * Set the `{}` field.\n", f.name))
            }
            _ => out.push_str(&format!(
                "     * Replace the `{}` field: the old value is released, the new one moved in.\n",
                f.name
            )),
        }
        out.push_str("     */\n");
        out.push_str(&format!("    public function {}($value): self\n", set));
        out.push_str("    {\n");
        for l in body {
            out.push_str(&format!("        {}\n", l));
        }
        out.push_str("        return $this;\n");
        out.push_str("    }\n\n");
        n += 1;
    }
    n
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
    out.push_str(&format!("    public function {}({})\n", php_name, params));
    out.push_str("    {\n");

    emit_callback_register_lines(out, &user_args);

    // Inspect args[0]: Owned ⇒ C ABI takes the receiver by value
    // (`<ClassName>`); Ref/Ptr ⇒ takes a pointer (`<ClassName>*`).
    // PHP FFI does NOT auto-dereference, so passing `\FFI::addr(...)`
    // where a value is expected either crashes or passes garbage —
    // same shape as the Zig 5.1 fix. Mirrors JVM/CLR `self_by_value`.
    let self_by_value = f
        .args
        .first()
        .map(|a| matches!(a.ref_kind, super::super::ir::ArgRefKind::Owned))
        .unwrap_or(false);
    // A by-value receiver is MOVED out through `intoRaw()`: the wrapper
    // stops owning it (a field view hands over a copy instead, so its
    // owner keeps the field).
    let self_expr = if self_by_value {
        "$this->intoRaw()"
    } else {
        "\\FFI::addr($this->ptr)"
    };

    // Callback-wrapper args bind the `<c_name>Struct` C symbol (whole
    // wrapper struct by value — matches the cdata registerCallback
    // returns). The raw `<c_name>` takes a bare fn ptr at the C ABI.
    let mut call = format!(
        "Azul::lib()->{}({}",
        super::super::managed_host_invoker::managed_c_symbol(f),
        self_expr
    );
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
///
/// The derive exports take the receiver as their FIRST argument whatever
/// api.json calls it (`instance`, not `self`), so it is dropped here rather
/// than by name: `clone($instance)` passed the receiver twice. `clone`
/// wraps the deep copy in a new owning wrapper; `toString` decodes the
/// returned AzString into a PHP string and frees it.
fn emit_instance_method_alias(out: &mut String, f: &FunctionDef, php_name: &str) {
    let user_args: Vec<&super::super::ir::FunctionArg> = f.args.iter().skip(1).collect();
    let params = render_php_params(&user_args);
    let user_call_args = render_call_args(&user_args);
    let returns_self = f.return_type.as_deref().map(str::trim) == Some(f.class_name.as_str());
    let returns_string = f.kind == FunctionKind::DebugToString;

    out.push_str("    /**\n");
    out.push_str(&format!(
        "     * Idiomatic alias dispatching to `Azul::lib()->{}`.\n",
        f.c_name
    ));
    out.push_str("     */\n");
    let hint = if returns_string {
        ": string"
    } else if returns_self {
        ": self"
    } else {
        ""
    };
    out.push_str(&format!("    public function {}({}){}\n", php_name, params, hint));
    out.push_str("    {\n");

    emit_callback_register_lines(out, &user_args);

    let mut call = format!(
        "Azul::lib()->{}(\\FFI::addr($this->ptr)",
        super::super::managed_host_invoker::managed_c_symbol(f)
    );
    if !user_call_args.is_empty() {
        call.push_str(", ");
        call.push_str(&user_call_args);
    }
    call.push(')');

    if f.return_type.is_none() {
        out.push_str(&format!("        {};\n", call));
    } else if returns_string {
        out.push_str(&format!("        $__s = {};\n", call));
        out.push_str("        $__out = Azul::readString($__s);\n");
        out.push_str("        Azul::lib()->AzString_delete(\\FFI::addr($__s));\n");
        out.push_str("        return $__out;\n");
    } else if returns_self {
        out.push_str(&format!("        return new self({});\n", call));
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
    out.push_str(&format!("     * Wraps `Azul::lib()->{}`.\n", f.c_name));
    if returns_self {
        out.push_str(&"     *\n     * @return self instance wrapping the returned FFI cdata.\n".to_string());
    }
    out.push_str("     */\n");
    let return_hint = if returns_self { ": self" } else { "" };
    out.push_str(&format!(
        "    public static function {}({}){}\n",
        php_name, params, return_hint
    ));
    out.push_str("    {\n");

    emit_callback_register_lines(out, &user_args);

    let call = format!(
        "Azul::lib()->{}({})",
        super::super::managed_host_invoker::managed_c_symbol(f),
        user_call_args
    );
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

/// Filter the implicit receiver out of a function's arguments — the
/// receiver is supplied by `$this` for instance methods, and is absent
/// entirely for static factories.
fn user_args(f: &FunctionDef) -> Vec<&super::super::ir::FunctionArg> {
    f.args.iter().filter(|a| !f.is_receiver_arg(a)).collect()
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

/// For every arg whose IR `callback_info` is in the host-invoker
/// allowlist, emit `$name = Azul::registerCallback('Wrapper', $name);`
/// before the C call so the user can pass a plain PHP closure and have
/// the host-invoker plumbing wire it up automatically.
fn emit_callback_register_lines(out: &mut String, args: &[&super::super::ir::FunctionArg]) {
    for a in args {
        let Some(cb) = a.callback_info.as_ref() else {
            continue;
        };
        let wrapper = cb.callback_wrapper_name.as_str();
        let name = sanitize_php_identifier(&a.name);
        out.push_str(&format!(
            "        ${n} = \\Azul\\Azul::registerCallback('{w}', ${n});\n",
            n = name,
            w = wrapper
        ));
    }
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

/// Pick a safe PHP class name.
///
/// Most types just drop the `Az` prefix: `AzApp` → `Azul\App`, since
/// the wrapper lives inside `namespace Azul`. The exception is when the
/// unprefixed name collides with a PHP reserved word — `Void`, `String`,
/// `Int`, `Float`, `Bool`, etc. all overlap with PHP 7+/8+ scalar
/// type-hint syntax and PHP rejects them as class names case-insensitively.
///
/// For those, we keep the `Az` prefix verbatim so the host-language
/// alias is `Azul\AzString` instead of `Azul\String`. This is
/// per-language handling — Lua/Node/Ruby get to drop the prefix because
/// their language semantics allow shadowing built-in `String`/`Void`.
/// (See `examples/node/hello-world.js` for the JS-side comment about
/// shadowing the native `String` constructor.)
fn sanitize_class_name(raw: &str) -> String {
    if php_class_name_is_reserved(raw) {
        format!("Az{}", raw)
    } else {
        raw.to_string()
    }
}

/// Return `true` when `name` is a PHP reserved word that cannot appear
/// at a class declaration site, case-insensitively. Mirrors the PHP
/// keyword list in `autofix::reserved_keywords` plus the modern scalar
/// type aliases (PHP 7+/8+) that override class names at parse time.
fn php_class_name_is_reserved(name: &str) -> bool {
    let lower = name.to_lowercase();
    matches!(
        lower.as_str(),
        // Modern scalar type aliases (PHP 7+/8+) — these *cannot* be
        // class names because the parser treats them as type-hint syntax.
        "void"
            | "int"
            | "integer"
            | "float"
            | "double"
            | "bool"
            | "boolean"
            | "string"
            | "iterable"
            | "object"
            | "mixed"
            | "never"
            | "true"
            | "false"
            | "null"
            // Namespace self-references.
            | "self"
            | "parent"
            | "static"
            // Type-hint aliases that are also reserved.
            | "callable"
            | "array"
            | "list"
            // PHP 8.1 enum keyword.
            | "enum"
            // FULL reserved-keyword set: PHP rejects EVERY keyword as a
            // class name at parse time ("unexpected token X, expecting
            // identifier"), not just the scalar type aliases above. The
            // Switch widget (api.json class `Switch`) was the first real
            // collision: `final class Switch` broke the whole 100k-line
            // Azul.php with a parse error. Mirrors sanitize_php_identifier.
            | "abstract" | "and" | "as" | "break" | "case" | "catch" | "class"
            | "clone" | "const" | "continue" | "declare" | "default" | "die"
            | "do" | "echo" | "else" | "elseif" | "empty" | "enddeclare"
            | "endfor" | "endforeach" | "endif" | "endswitch" | "endwhile"
            | "eval" | "exit" | "extends" | "final" | "finally" | "fn" | "for"
            | "foreach" | "function" | "global" | "goto" | "if" | "implements"
            | "include" | "include_once" | "instanceof" | "insteadof"
            | "interface" | "isset" | "match" | "namespace" | "new" | "or"
            | "print" | "private" | "protected" | "public" | "require"
            | "require_once" | "return" | "switch" | "throw" | "trait" | "try"
            | "unset" | "use" | "var" | "while" | "xor" | "yield"
    )
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
        | "include" | "require" | "include_once" | "require_once" | "match" | "fn" | "array"
        | "callable" | "bool" | "int" | "float" | "string" | "void" | "iterable" | "object"
        | "mixed" | "never" | "self" | "parent" | "true" | "false" | "null" => {
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
