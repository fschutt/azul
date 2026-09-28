//! The *derive surface* of the Java binding.
//!
//! An api.json `derive` / `custom_impls` list makes libazul export a fixed
//! set of entry points for a type — `_toDbgString`, `_partialEq`, `_hash`,
//! `_cmp`, `_partialCmp`, `_deepCopy`, `_createDefault`, `_delete`. Java has
//! a name for almost every one of them (`toString()`, `equals(Object)`,
//! `hashCode()`, `Comparable.compareTo`), so this module spells them that way
//! and routes each one through the exported C function. Nothing here
//! reimplements a comparison, a hash or a debug rendering in Java: the answer
//! always comes from libazul, which is the only place that knows the type's
//! Rust semantics.
//!
//! ## Why this lives on the FFI value class
//!
//! [`wrappers`](super::wrappers) emits an `AutoCloseable` handle class for the
//! ~670 types that own native resources or carry instance methods. Every other
//! type — the 400+ POD structs, all ~590 tagged unions, all 119 monomorphized
//! `CssPropertyValue<T>` aliases — is represented in Java *only* by the JNA
//! `Structure` / `Union` class `types.rs` emits. That class is the value, so
//! that class is where its derives belong. Types that also have a wrapper get
//! the surface in both places; the two never disagree, because both call the
//! same C export.
//!
//! Overriding `equals` / `hashCode` / `toString` on a JNA `Structure` is safe:
//! JNA's own `Structure.StructureSet` compares by class + size + pointer
//! rather than through `equals`, precisely so subclasses may override them.
//!
//! ## What is deliberately NOT surfaced
//!
//! * `delete()` on a borrowed slice (`TypeCategory::VecRef`) — the value is `{ptr, len}` over
//!   memory the CALLER owns and its `_delete` is a no-op `drop_in_place`, so a `delete()` there
//!   would only teach user code that it owns someone else's buffer. Reading one (debug, equality,
//!   ordering, hashing) and copying the borrow are harmless and stay.
//! * `_partialCmp` on a type that also exports `_cmp` — a total order answers both questions, and
//!   `Comparable` can only carry one.
//! * Unit enums — they lower to a Java `enum`, which already has value equality, hashing and
//!   ordering of its own and no pointer to hand the C ABI.

use std::collections::BTreeSet;

use anyhow::Result;

use super::{
    super::{
        config::CodegenConfig,
        generator::CodeBuilder,
        ir::{ArgRefKind, CodegenIR, FunctionDef, FunctionKind, TypeCategory},
        managed_host_invoker::managed_c_symbol,
        managed_lang_helpers::has_wrapper_class,
    },
    emit_file, ffi_type_name,
    functions::{native_class_for_func, should_emit_function},
    javadoc_escape, map_jvm_type, map_jvm_type_byvalue, sanitize_identifier,
    wrappers::{idiomatic_method_name, wrapper_class_name},
};

// ============================================================================
// Name hygiene
// ============================================================================

/// Members that already exist on a generated FFI value class: what
/// `java.lang.Object` and JNA's `Structure` / `Union` define, what
/// [`emit_value_derives`] adds, and what `types.rs` adds to the Vec / Option /
/// Result shapes. A forwarder whose idiomatic name lands on one of these gets
/// an underscore, so it can never collide with — or illegally hide — a member
/// of the same name and a different signature.
const RESERVED_MEMBER_NAMES: &[&str] = &[
    // java.lang.Object
    "clone",
    "equals",
    "finalize",
    "getClass",
    "hashCode",
    "notify",
    "notifyAll",
    "toString",
    "wait",
    // com.sun.jna.Structure / com.sun.jna.Union
    "autoRead",
    "autoWrite",
    "clear",
    "dataEquals",
    "ensureAllocated",
    "fields",
    "getAutoRead",
    "getAutoWrite",
    "getFieldList",
    "getFieldOrder",
    "getPointer",
    "getStringEncoding",
    "getTypedValue",
    "newInstance",
    "read",
    "readField",
    "setAlignType",
    "setAutoRead",
    "setAutoWrite",
    "setStringEncoding",
    "setType",
    "size",
    "toArray",
    "useMemory",
    "write",
    "writeField",
    // added by this module
    "compareTo",
    "createDefault",
    "deepCopy",
    "delete",
    // added by types.rs to the Vec / Option / Result shapes
    "isErr",
    "isOk",
    "toByteArray",
    "toDoubleArray",
    "toFloatArray",
    "toIntArray",
    "toList",
    "toLongArray",
    "toNullable",
    "toShortArray",
    "unwrap",
];

/// The Java name a member forwarder on an FFI value class takes: the same
/// rule the wrapper layer uses, plus the FFI class's own inherited surface.
fn ffi_member_name(method_name: &str) -> String {
    let camel = idiomatic_method_name(method_name);
    if RESERVED_MEMBER_NAMES.contains(&camel.as_str()) {
        format!("{}_", camel)
    } else {
        camel
    }
}

// ============================================================================
// IR lookups
// ============================================================================

/// The C entry point of `kind` on `class_name`, but only when the Java FFI
/// layer actually declared it. Gating on the same predicate that emits the
/// `public static native` line is what makes every call site below compile by
/// construction: a symbol this returns is a symbol `AzulNative<Module>` has.
fn declared_fn<'a>(
    class_name: &str,
    kind: FunctionKind,
    ir: &'a CodegenIR,
    config: &CodegenConfig,
) -> Option<&'a FunctionDef> {
    ir.functions
        .iter()
        .find(|f| f.class_name == class_name && f.kind == kind)
        .filter(|f| should_emit_function(*f, ir, config))
}

/// Is this type a borrowed slice — `{ptr, len}` over memory the CALLER owns?
/// Its `_clone` copies the BORROW rather than the buffer, and its `_delete` is
/// a no-op `drop_in_place`: the first is worth surfacing (with the right
/// doc), the second is not.
fn is_borrowed_slice(class_name: &str, ir: &CodegenIR) -> bool {
    ir.find_struct(class_name)
        .is_some_and(|s| s.category == TypeCategory::VecRef)
}

/// The ordering entry point to route `Comparable.compareTo` through, plus
/// whether it is a TOTAL order (`_cmp`, which always answers) or a partial one
/// (`_partialCmp`, which may report "not ordered").
fn ordering_fn<'a>(
    class_name: &str,
    ir: &'a CodegenIR,
    config: &CodegenConfig,
) -> Option<(&'a FunctionDef, bool)> {
    // `(a, b) -> u8` is the shape both ordering exports have; a gate here
    // rather than in the emitter is what keeps the `implements Comparable<..>`
    // clause and the `compareTo` body from ever disagreeing.
    fn two_args(f: &&FunctionDef) -> bool {
        f.args.len() == 2
    }
    if let Some(f) = declared_fn(class_name, FunctionKind::Cmp, ir, config).filter(two_args) {
        return Some((f, true));
    }
    declared_fn(class_name, FunctionKind::PartialCmp, ir, config)
        .filter(two_args)
        .map(|f| (f, false))
}

/// `<NativeClass>.INSTANCE.<symbol>` for `func` — the exact spelling the
/// per-module JNA class declared, so a call site can never drift from the
/// declaration.
fn native_call(func: &FunctionDef, ir: &CodegenIR) -> String {
    format!(
        "{}.INSTANCE.{}",
        native_class_for_func(func, ir),
        managed_c_symbol(func)
    )
}

/// The FFI class of the engine's string type plus the call that frees one.
/// Every `_toDbgString` hands back a freshly allocated `AzString`; decoding it
/// without freeing it again leaks the UTF-8 buffer once per `toString()` call.
/// Resolved from `TypeCategory::String`, never from the type's name.
fn string_free(ir: &CodegenIR) -> Option<(&str, String, String)> {
    let s = ir
        .structs
        .iter()
        .find(|s| s.category == TypeCategory::String)?;
    let del = ir
        .functions
        .iter()
        .find(|f| f.class_name == s.name && f.kind == FunctionKind::Delete)?;
    Some((
        s.name.as_str(),
        ffi_type_name(&s.name),
        native_call(del, ir),
    ))
}

// ============================================================================
// Class-declaration fragment
// ============================================================================

/// The `implements` fragment an FFI value class needs for its derive surface,
/// or `""`. Kept separate from [`emit_value_derives`] because the interface
/// list is part of the class's opening line, which `types.rs` writes.
pub(super) fn comparable_clause(
    class_name: &str,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> String {
    match ordering_fn(class_name, ir, config) {
        Some(_) => format!(" implements Comparable<{}>", ffi_type_name(class_name)),
        None => String::new(),
    }
}

/// Same question for a wrapper class, which lists its interfaces itself and
/// carries the wrapper's (unprefixed) name.
pub(super) fn wrapper_comparable_interface(
    class_name: &str,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Option<String> {
    ordering_fn(class_name, ir, config)
        .map(|_| format!("Comparable<{}>", wrapper_class_name(class_name)))
}

// ============================================================================
// The derive surface on an FFI value class
// ============================================================================

/// Emit every derive the type's api.json traits allow onto the `Az<X>` JNA
/// `Structure` / `Union` class. Must be called BEFORE the nested
/// `ByValue` / `ByReference` classes: the generated members belong to the
/// outer class body.
pub(super) fn emit_value_derives(
    b: &mut CodeBuilder,
    class_name: &str,
    ir: &CodegenIR,
    config: &CodegenConfig,
) {
    let ffi = ffi_type_name(class_name);
    let borrowed = is_borrowed_slice(class_name, ir);

    if let Some(f) = declared_fn(class_name, FunctionKind::DebugToString, ir, config) {
        emit_to_string(b, f, ir);
    }

    // `(a, b) -> bool` and `(instance) -> u64`: gated here, because a class
    // that emits `equals` MUST also emit a value-consistent `hashCode` — the
    // fallback below can only fire if the decision is taken in one place.
    let eq =
        declared_fn(class_name, FunctionKind::PartialEq, ir, config).filter(|f| f.args.len() == 2);
    let hash = declared_fn(class_name, FunctionKind::Hash, ir, config).filter(|f| {
        f.args.len() == 1
            && f.return_type.as_ref().map(|r| map_jvm_type_byvalue(r, ir))
                == Some("long".to_string())
    });
    if let Some(f) = eq {
        emit_equals(b, &ffi, f, ir);
    }
    if let Some(f) = hash {
        emit_hash_code(b, f, ir);
    } else if eq.is_some() {
        // equals() compares VALUES and this type exports no value hash. The
        // only hash that keeps "equal values hash equal" is a constant: the
        // memory address differs between two equal values.
        b.line("/** Constant: equal values must hash equal, and the type has no value hash. */");
        b.line("@Override");
        b.line("public int hashCode() {");
        b.indent();
        b.line("return 0;");
        b.dedent();
        b.line("}");
        b.blank();
    }

    if let Some((f, total)) = ordering_fn(class_name, ir, config) {
        emit_compare_to(b, &ffi, f, ir, total);
    }
    if let Some(f) = declared_fn(class_name, FunctionKind::DeepCopy, ir, config) {
        emit_deep_copy(b, f, ir, borrowed);
    }
    if let Some(f) = declared_fn(class_name, FunctionKind::Default, ir, config) {
        emit_create_default(b, f, ir);
    }
    // A borrowed slice does not own its buffer — see the module docs.
    if !borrowed {
        if let Some(f) = declared_fn(class_name, FunctionKind::Delete, ir, config) {
            emit_delete(b, f, ir);
        }
    }
}

/// `toString()` through `Az<X>_toDbgString`.
///
/// The C ABI reads the value out of native memory, so the Java-side fields are
/// flushed with `write()` first — a JNA `Structure` only syncs itself when JNA
/// itself passes it, and we hand over the raw `Pointer`.
fn emit_to_string(b: &mut CodeBuilder, f: &FunctionDef, ir: &CodegenIR) {
    let Some((string_ty, string_ffi, free_call)) = string_free(ir) else {
        return;
    };
    // The decoder below reads the engine's string layout out of the returned
    // value, so it is only correct when the entry point really returns that
    // type. (A unit enum's `_toDbgString` returns the enum: unreachable from
    // here — a unit enum lowers to a Java enum, not to a value class — but
    // the check keeps the two facts tied together.)
    if f.return_type.as_deref().map(str::trim) != Some(string_ty) {
        return;
    }
    b.line("/**");
    b.line(&format!(
        " * Debug representation, routed through {}.",
        javadoc_escape(&managed_c_symbol(f))
    ));
    b.line(" */");
    b.line("@Override");
    b.line("public java.lang.String toString() {");
    b.indent();
    b.line("if (getPointer() == null) return super.toString();");
    b.line("write();");
    b.line(&format!(
        "{}.ByValue __s = {}(getPointer());",
        string_ffi,
        native_call(f, ir)
    ));
    b.line("__s.write();");
    b.line("Pointer __sp = __s.getPointer();");
    // The engine's string is `{ vec: U8Vec }` and a U8Vec is
    // `{ ptr, len, cap, destructor }`: offset 0 is the UTF-8 buffer, offset 8
    // its byte length.
    b.line("Pointer __vec = __sp.getPointer(0);");
    b.line("long __len = __sp.getLong(8);");
    b.line("java.lang.String __out = \"\";");
    b.line("if (__vec != null && __len > 0) {");
    b.indent();
    b.line(
        "__out = new java.lang.String(__vec.getByteArray(0, (int) __len), \
         java.nio.charset.StandardCharsets.UTF_8);",
    );
    b.dedent();
    b.line("}");
    b.line(&format!("{}(__sp);", free_call));
    b.line("return __out;");
    b.dedent();
    b.line("}");
    b.blank();
}

/// `equals(Object)` through `Az<X>_partialEq`.
fn emit_equals(b: &mut CodeBuilder, ffi: &str, f: &FunctionDef, ir: &CodegenIR) {
    b.line("/**");
    b.line(&format!(
        " * Value equality, routed through {}.",
        javadoc_escape(&managed_c_symbol(f))
    ));
    b.line(" */");
    b.line("@Override");
    b.line("public boolean equals(Object other) {");
    b.indent();
    b.line(&format!("if (!(other instanceof {})) return false;", ffi));
    b.line(&format!("{} __o = ({}) other;", ffi, ffi));
    b.line(
        "if (getPointer() == null || __o.getPointer() == null) return getPointer() == \
         __o.getPointer();",
    );
    b.line("write();");
    b.line("__o.write();");
    // C's `bool` is one byte and JNA carries it as `byte` here (see
    // `map_jvm_type`): compare against zero rather than `== true`.
    b.line(&format!(
        "return {}(getPointer(), __o.getPointer()) != 0;",
        native_call(f, ir)
    ));
    b.dedent();
    b.line("}");
    b.blank();
}

/// `hashCode()` through `Az<X>_hash` (a `u64`, folded into Java's `int`).
fn emit_hash_code(b: &mut CodeBuilder, f: &FunctionDef, ir: &CodegenIR) {
    b.line("/**");
    b.line(&format!(
        " * Value hash, routed through {}.",
        javadoc_escape(&managed_c_symbol(f))
    ));
    b.line(" */");
    b.line("@Override");
    b.line("public int hashCode() {");
    b.indent();
    b.line("if (getPointer() == null) return 0;");
    b.line("write();");
    b.line(&format!("long __h = {}(getPointer());", native_call(f, ir)));
    b.line("return (int) (__h ^ (__h >>> 32));");
    b.dedent();
    b.line("}");
    b.blank();
}

/// `Comparable.compareTo` through `Az<X>_cmp` (total) or `Az<X>_partialCmp`.
///
/// Both return the same one-byte encoding libazul uses everywhere:
/// `0 = Less`, `1 = Equal`, `2 = Greater`, anything else = "not ordered"
/// (only `_partialCmp` can produce it — that is what makes it partial).
fn emit_compare_to(
    b: &mut CodeBuilder,
    ffi: &str,
    f: &FunctionDef,
    ir: &CodegenIR,
    total_order: bool,
) {
    b.line("/**");
    b.line(&format!(
        " * Ordering, routed through {}.",
        javadoc_escape(&managed_c_symbol(f))
    ));
    if !total_order {
        b.line(" * <p>");
        b.line(" * The type has a PARTIAL order only: two values can be unordered");
        b.line(" * (a NaN field, say), which Comparable cannot express — that case");
        b.line(" * throws rather than claiming the values are equal.");
    }
    b.line(" */");
    b.line("@Override");
    b.line(&format!("public int compareTo({} other) {{", ffi));
    b.indent();
    b.line("if (other == null) throw new NullPointerException(\"compareTo(null)\");");
    b.line("if (getPointer() == null || other.getPointer() == null) return 0;");
    b.line("write();");
    b.line("other.write();");
    b.line(&format!(
        "byte __ord = {}(getPointer(), other.getPointer());",
        native_call(f, ir)
    ));
    b.line("if (__ord == 0) return -1;");
    b.line("if (__ord == 2) return 1;");
    if total_order {
        b.line("return 0;");
    } else {
        b.line("if (__ord == 1) return 0;");
        b.line(
            "throw new IllegalArgumentException(\"the two values are not ordered relative to \
             each other.\");",
        );
    }
    b.dedent();
    b.line("}");
    b.blank();
}

/// `deepCopy()` through `Az<X>_clone`.
///
/// `borrowed_slice`: the value is a `{ptr, len}` view, so the C `_clone`
/// duplicates the VIEW and not the buffer behind it. Same call, different
/// promise — the doc has to say which one the caller gets.
fn emit_deep_copy(b: &mut CodeBuilder, f: &FunctionDef, ir: &CodegenIR, borrowed_slice: bool) {
    let Some(ret) = f.return_type.as_ref().map(|r| map_jvm_type_byvalue(r, ir)) else {
        return;
    };
    // `(instance) -> Self`.
    if f.args.len() != 1 {
        return;
    }
    b.line("/**");
    b.line(&format!(
        " * Copy, routed through {}.",
        javadoc_escape(&managed_c_symbol(f))
    ));
    if borrowed_slice {
        b.line(" * <p>");
        b.line(" * This value is a borrowed {ptr, len} view: the copy is a second");
        b.line(" * view of the SAME buffer, which the caller still owns. It must not");
        b.line(" * outlive that buffer, and nothing frees it.");
    } else {
        b.line(" * <p>");
        b.line(" * A deep copy: it owns its own heap allocations and outlives this");
        b.line(" * value; free it with delete().");
    }
    b.line(" */");
    b.line(&format!("public {} deepCopy() {{", ret));
    b.indent();
    b.line("write();");
    b.line(&format!("return {}(getPointer());", native_call(f, ir)));
    b.dedent();
    b.line("}");
    b.blank();
}

/// `static createDefault()` through `Az<X>_createDefault`.
fn emit_create_default(b: &mut CodeBuilder, f: &FunctionDef, ir: &CodegenIR) {
    let Some(ret) = f.return_type.as_ref().map(|r| map_jvm_type_byvalue(r, ir)) else {
        return;
    };
    // `() -> Self`.
    if !f.args.is_empty() {
        return;
    }
    b.line("/**");
    b.line(&format!(
        " * The type's Default value, routed through {}.",
        javadoc_escape(&managed_c_symbol(f))
    ));
    b.line(" */");
    b.line(&format!("public static {} createDefault() {{", ret));
    b.indent();
    b.line(&format!("return {}();", native_call(f, ir)));
    b.dedent();
    b.line("}");
    b.blank();
}

/// `delete()` through `Az<X>_delete`.
///
/// Explicit, never automatic: an FFI value class is just a view on some
/// memory, and the same bytes are routinely handed out borrowed (a callback
/// argument, an element read out of a Vec, a field overlay). A finalizer here
/// would free memory this value does not own.
fn emit_delete(b: &mut CodeBuilder, f: &FunctionDef, ir: &CodegenIR) {
    // `(instance) -> ()`.
    if f.args.len() != 1 || f.return_type.is_some() {
        return;
    }
    b.line("/**");
    b.line(&format!(
        " * Free the native resources this value owns ({}).",
        javadoc_escape(&managed_c_symbol(f))
    ));
    b.line(" * <p>");
    b.line(" * Call it exactly once, and only on a value you own: never on one the");
    b.line(" * engine lent you, and never on one you already passed to the C ABI by");
    b.line(" * value (that transferred ownership).");
    b.line(" */");
    b.line("public void delete() {");
    b.indent();
    b.line("if (getPointer() == null) return;");
    b.line("write();");
    b.line(&format!("{}(getPointer());", native_call(f, ir)));
    b.dedent();
    b.line("}");
    b.blank();
}

/// `Comparable.compareTo` on a WRAPPER class.
///
/// Same C export as [`emit_compare_to`], but a wrapper is a handle rather
/// than the value: it keeps the native pointer in a field, so there is
/// nothing to flush and nothing to overlay.
pub(super) fn emit_wrapper_compare_to(
    b: &mut CodeBuilder,
    class_name: &str,
    wrapper: &str,
    ir: &CodegenIR,
    config: &CodegenConfig,
) {
    let Some((f, total_order)) = ordering_fn(class_name, ir, config) else {
        return;
    };
    b.line("/**");
    b.line(&format!(
        " * Ordering, routed through {}.",
        javadoc_escape(&managed_c_symbol(f))
    ));
    if !total_order {
        b.line(" * <p>");
        b.line(" * The type has a PARTIAL order only: two values can be unordered,");
        b.line(" * which Comparable cannot express — that case throws rather than");
        b.line(" * claiming the values are equal.");
    }
    b.line(" */");
    b.line("@Override");
    b.line(&format!("public int compareTo({} other) {{", wrapper));
    b.indent();
    b.line("if (other == null) throw new NullPointerException(\"compareTo(null)\");");
    b.line("if (this.ptr == null || other.ptr == null) return 0;");
    b.line(&format!(
        "byte __ord = {}(this.ptr, other.ptr);",
        native_call(f, ir)
    ));
    b.line("if (__ord == 0) return -1;");
    b.line("if (__ord == 2) return 1;");
    if total_order {
        b.line("return 0;");
    } else {
        b.line("if (__ord == 1) return 0;");
        b.line(
            "throw new IllegalArgumentException(\"the two values are not ordered relative to \
             each other.\");",
        );
    }
    b.dedent();
    b.line("}");
    b.blank();
}

// ============================================================================
// api.json members of a type with no wrapper class
// ============================================================================

/// Surface the api.json-declared constructors, static methods and instance
/// methods of a type that has no wrapper class.
///
/// ~1400 types never reach [`wrappers`](super::wrappers): a POD struct with a
/// factory but no `_delete` and no instance method (`LayoutSize.zero()`,
/// `Uuid.v4()`), every tagged union (`CssProperty.constDisplay(...)`,
/// `SvgPathElement.getBounds(...)`) and every monomorphized alias. Their
/// members are emitted here as `static` forwarders on the FFI value class:
/// the signature mirrors the `public static native` declaration exactly — the
/// receiver included, since a value handed over by pointer and one handed over
/// by value are different calls — so the forwarding call type-checks by
/// construction.
pub(super) fn emit_member_facade(
    b: &mut CodeBuilder,
    class_name: &str,
    ir: &CodegenIR,
    config: &CodegenConfig,
) {
    // A type WITH a wrapper surfaces its members there, in idiomatic form
    // (auto-converted strings, wrapper-typed args, Optional returns).
    if has_wrapper_class(class_name, ir) {
        return;
    }
    // Two api.json entries can idiomise to one Java name; emit the first and
    // skip the rest rather than emitting a duplicate the compiler rejects.
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for func in ir.functions_for_class(class_name) {
        if !func.kind.is_api_function() || !should_emit_function(func, ir, config) {
            continue;
        }
        let name = ffi_member_name(&func.method_name);
        let params: Vec<(String, String)> = func
            .args
            .iter()
            .map(|a| {
                let jt = match a.ref_kind {
                    ArgRefKind::Owned => map_jvm_type_byvalue(&a.type_name, ir),
                    // Pass-by-pointer at the C level — same as the native
                    // declaration, which JNA binds as a raw `Pointer`.
                    ArgRefKind::Ref | ArgRefKind::RefMut | ArgRefKind::Ptr | ArgRefKind::PtrMut => {
                        "Pointer".to_string()
                    }
                };
                (jt, sanitize_identifier(&a.name))
            })
            .collect();
        let signature = format!(
            "{}({})",
            name,
            params
                .iter()
                .map(|(t, _)| t.as_str())
                .collect::<Vec<_>>()
                .join(",")
        );
        if !seen.insert(signature) {
            continue;
        }
        let ret = func
            .return_type
            .as_ref()
            .map(|r| map_jvm_type_byvalue(r, ir))
            .unwrap_or_else(|| "void".to_string());

        if !func.doc.is_empty() {
            b.line("/**");
            for d in &func.doc {
                b.line(&format!(" * {}", javadoc_escape(d)));
            }
            b.line(" */");
        }
        b.line(&format!(
            "public static {} {}({}) {{",
            ret,
            name,
            params
                .iter()
                .map(|(t, n)| format!("{} {}", t, n))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        b.indent();
        let call = format!(
            "{}({})",
            native_call(func, ir),
            params
                .iter()
                .map(|(_, n)| n.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
        if ret == "void" {
            b.line(&format!("{};", call));
        } else {
            b.line(&format!("return {};", call));
        }
        b.dedent();
        b.line("}");
        b.blank();
    }
}

// ============================================================================
// api.json constants
// ============================================================================

/// The api.json constants declared on `class_name`, as (Java name, Java type,
/// Java literal, doc).
fn constants_of<'a>(
    class_name: &str,
    ir: &'a CodegenIR,
) -> Vec<(String, String, String, &'a [String])> {
    let prefix = format!("{}_", class_name);
    ir.constants
        .iter()
        .filter_map(|c| {
            let name = &c.member_name();
            c.name.strip_prefix(&prefix)?;
            let java_type = map_jvm_type(&c.type_name, ir);
            let literal = constant_literal(&c.value, &java_type);
            Some((
                sanitize_identifier(name),
                java_type,
                literal,
                c.doc.as_slice(),
            ))
        })
        .collect()
}

/// api.json gives a constant's value as the literal its C type wants
/// (`0x0D5B`, `0`). Java takes the same spelling for `int`, needs the `L`
/// suffix once the value no longer fits one, and only auto-narrows DECIMAL
/// constants — so a hex value assigned to a `byte` / `short` carries the cast.
fn constant_literal(value: &str, java_type: &str) -> String {
    let v = value.trim();
    match java_type {
        "byte" | "short" | "char" => format!("({}) {}", java_type, v),
        "long" if !v.ends_with('L') && !v.ends_with('l') => format!("{}L", v),
        _ => v.to_string(),
    }
}

/// Emit `public static final` fields for the api.json constants of
/// `class_name`. Called from the wrapper emitter so the constants sit on the
/// class they belong to (`GlContextPtr.ACCUM_ALPHA_BITS`).
pub(super) fn emit_constants(b: &mut CodeBuilder, class_name: &str, ir: &CodegenIR) {
    let constants = constants_of(class_name, ir);
    if constants.is_empty() {
        return;
    }
    b.line(&format!(
        "// The {} constants api.json declares on this class.",
        constants.len()
    ));
    for (name, java_type, literal, doc) in &constants {
        for d in doc.iter() {
            b.line(&format!("/** {} */", javadoc_escape(d)));
        }
        b.line(&format!(
            "public static final {} {} = {};",
            java_type, name, literal
        ));
    }
    b.blank();
}

/// A holder file for the constants of a class that has no wrapper to carry
/// them. None exist in the current api.json — every constant-declaring class
/// has a wrapper — but a new one must not silently drop its constants.
pub(super) fn emit_orphan_constant_files(
    out: &mut String,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    let mut owners: BTreeSet<&str> = BTreeSet::new();
    for c in &ir.constants {
        if let Some((class, _)) = c.name.split_once('_') {
            owners.insert(class);
        }
    }
    for class in owners {
        if !config.should_include_type(class) || has_wrapper_class(class, ir) {
            continue;
        }
        if constants_of(class, ir).is_empty() {
            continue;
        }
        let holder = format!("{}Constants", sanitize_identifier(class));
        let chunk = emit_file(
            &format!("{}.java", holder),
            |b| {
                b.line(&format!(
                    "/** The api.json constants of {} — the type itself has no wrapper class. */",
                    class
                ));
                b.line(&format!("public final class {} {{", holder));
                b.indent();
                b.line(&format!("private {}() {{}}", holder));
                b.blank();
                emit_constants(b, class, ir);
                b.dedent();
                b.line("}");
                Ok(())
            },
            config,
        )?;
        out.push_str(&chunk);
    }
    Ok(())
}
