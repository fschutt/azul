//! The value surface of every FFI class `wrappers.rs` does not wrap.
//!
//! `wrappers.rs` emits a hand-shaped `class Dom internal constructor(ptr)`
//! for every type `has_wrapper_class` accepts — a struct with a `_delete` or
//! at least one instance method. That predicate answers `false` for three
//! whole families, and each of them is nevertheless a REAL type with real
//! exports behind it:
//!
//! * tagged unions (`CssProperty`, `OptionDom`, `ResultXmlXmlError`, …) —
//!   `has_wrapper_class` looks them up with `ir.find_struct`, which never
//!   finds an enum;
//! * POD structs with no `_delete` and no methods (`ColorU`-shaped value
//!   types whose whole API is the `derive` list);
//! * the monomorphized generic aliases (`BoxDecorationBreakValue =
//!   CssPropertyValue<BoxDecorationBreak>`), which `find_struct` AND
//!   `find_enum` both miss — only `find_type_alias` knows them.
//!
//! For all three the JNA class `mod.rs` emits (`open class AzCssProperty :
//! Union()`, `open class AzColorU : Structure()`) IS the Kotlin-visible
//! value, so that is where their surface belongs:
//!
//! | api.json         | C export         | Kotlin                       |
//! |------------------|------------------|------------------------------|
//! | `Debug`          | `_toDbgString`   | `override fun toString()`    |
//! | `PartialEq`/`Eq` | `_partialEq`     | `override fun equals()`      |
//! | `Hash`           | `_hash`          | `override fun hashCode()`    |
//! | `Ord`            | `_cmp`           | `Comparable` + `compareTo()` |
//! | `PartialOrd`     | `_partialCmp`    | `partialCompareTo(): Int?`   |
//! | `Clone`          | `_clone`         | `deepCopy()`                 |
//! | `Default`        | `_createDefault` | companion `default()`        |
//! | `Drop`           | `_delete`        | `AutoCloseable` + `close()`  |
//!
//! Every one of them is a call into the exported C function — nothing is
//! reimplemented in Kotlin, so the binding can never disagree with the
//! engine about what two values' equality, order or debug text is.
//!
//! The api.json constructors, static methods and instance methods of the
//! same types land in the class's `companion object` as typed pass-throughs
//! (see [`emit_companion`] for why they are companion functions and not
//! members).

use std::collections::{BTreeMap, BTreeSet};

use super::{
    super::{
        config::CodegenConfig,
        generator::CodeBuilder,
        ir::{
            ArgRefKind, CodegenIR, ConstantDef, FunctionArg, FunctionDef, FunctionKind, StructDef,
            TypeCategory,
        },
        lang_java::functions::native_class_for_func,
        managed_host_invoker::managed_c_symbol,
        managed_lang_helpers::has_wrapper_class,
    },
    base_map_jvm_type, jvm_to_kotlin_primitive, kotlin_class_name, map_kt_owned, map_kt_return,
    sanitize_kt_identifier, should_emit_function,
    wrappers::{idiomatic_method_name, kdoc_escape},
};

/// The file-private helper every `toString()` below funnels through. It is
/// emitted once by [`emit_string_decoder`] rather than inlined ~1200 times.
const STRING_DECODER: &str = "azStringIntoKotlin";

// ============================================================================
// What the IR says this type can do
// ============================================================================

/// The exports of one class, bucketed by the capability they implement.
/// Every field holds the `FunctionDef` itself, so the call site reads the C
/// symbol, the owning module and the declared signature off the IR instead
/// of rebuilding any of them from the type's name.
#[derive(Default)]
struct Surface<'a> {
    debug: Option<&'a FunctionDef>,
    eq: Option<&'a FunctionDef>,
    hash: Option<&'a FunctionDef>,
    cmp: Option<&'a FunctionDef>,
    partial_cmp: Option<&'a FunctionDef>,
    deep_copy: Option<&'a FunctionDef>,
    default: Option<&'a FunctionDef>,
    delete: Option<&'a FunctionDef>,
    /// Constructors, static methods and instance methods api.json declares.
    api_fns: Vec<&'a FunctionDef>,
}

impl Surface<'_> {
    fn is_empty(&self) -> bool {
        self.debug.is_none()
            && self.eq.is_none()
            && self.hash.is_none()
            && self.cmp.is_none()
            && self.partial_cmp.is_none()
            && self.deep_copy.is_none()
            && self.default.is_none()
            && self.delete.is_none()
            && self.api_fns.is_empty()
    }
}

fn surface<'a>(ir_name: &str, ir: &'a CodegenIR, config: &CodegenConfig) -> Surface<'a> {
    let mut out = Surface::default();
    let string_decoder = has_string_decoder(ir, config);
    // Not `functions_for_class`: its signature ties the class name's
    // lifetime to the IR's, which would stop the borrows below outliving
    // this call.
    for f in ir.functions.iter().filter(|f| f.class_name == ir_name) {
        // `should_emit_function` is what decided whether the symbol got an
        // `@JvmStatic external fun` at all. Calling one it filtered out
        // would not compile, so the surface can never be wider than the
        // FFI layer underneath it.
        if !should_emit_function(f, ir, config) {
            continue;
        }
        match f.kind {
            // `Debug` needs the engine string decoded, which needs the
            // string type's own `_delete`. Without it there is nothing to
            // surface `toString()` through.
            FunctionKind::DebugToString if string_decoder => out.debug = Some(f),
            FunctionKind::DebugToString => {}
            FunctionKind::PartialEq => out.eq = Some(f),
            FunctionKind::Hash => out.hash = Some(f),
            FunctionKind::Cmp => out.cmp = Some(f),
            FunctionKind::PartialCmp => out.partial_cmp = Some(f),
            FunctionKind::DeepCopy => out.deep_copy = Some(f),
            FunctionKind::Default => out.default = Some(f),
            FunctionKind::Delete => out.delete = Some(f),
            FunctionKind::Constructor
            | FunctionKind::StaticMethod
            | FunctionKind::Method
            | FunctionKind::MethodMut => out.api_fns.push(f),
            // Variant constructors are the tagged union's own shape and are
            // emitted by `wrappers::emit_union_helper`, not here.
            FunctionKind::EnumVariantConstructor => {}
        }
    }
    out
}

/// The IR category of a type, or `None` for a monomorphized generic alias —
/// `find_struct` and `find_enum` both answer nothing for those, which is
/// exactly what makes them easy to forget.
fn category_of(ir_name: &str, ir: &CodegenIR) -> Option<TypeCategory> {
    ir.find_struct(ir_name)
        .map(|s| s.category)
        .or_else(|| ir.find_enum(ir_name).map(|e| e.category))
}

/// Does this FFI class carry the value surface at all?
///
/// Two exclusions, both about not growing a SECOND surface for one value:
///
/// * a type `wrappers.rs` wraps already has one, on the wrapper — emitting
///   `close()` here as well would give user code two ways to free the same
///   allocation;
/// * the destructor/clone callback types (`*VecDestructor`) are engine
///   plumbing that no binding hands to user code.
fn carries_surface(ir_name: &str, ir: &CodegenIR) -> bool {
    !has_wrapper_class(ir_name, ir)
        && !matches!(
            category_of(ir_name, ir),
            Some(TypeCategory::DestructorOrClone)
        )
}

/// The `_delete` this class may expose as `close()`, if any.
///
/// A borrowed slice (`VecRef`) is `{ptr, len}` over memory the CALLER owns:
/// its `_delete` is a `drop_in_place` of the borrow, so surfacing it would
/// hand user code a way to free someone else's buffer. It stays reachable
/// from the raw FFI layer and nowhere else.
fn free_fn<'a>(s: &Surface<'a>, ir_name: &str, ir: &CodegenIR) -> Option<&'a FunctionDef> {
    if matches!(category_of(ir_name, ir), Some(TypeCategory::VecRef)) {
        return None;
    }
    s.delete
}

/// The Kotlin type of a function's return, as the `external fun` declares it.
fn return_kt(f: &FunctionDef, ir: &CodegenIR) -> String {
    f.return_type
        .as_deref()
        .map(|r| map_kt_return(r, ir))
        .unwrap_or_else(|| "Unit".to_string())
}

/// `AzulNativeCss.AzColorU_partialEq` — the qualified call target for `f`.
fn call_target(f: &FunctionDef, ir: &CodegenIR) -> String {
    format!("{}.{}", native_class_for_func(f, ir), managed_c_symbol(f))
}

// ============================================================================
// Class header
// ============================================================================

/// The supertypes the value surface adds to an FFI class's header, in the
/// order they are written after `Structure()` / `Union()`.
pub(super) fn supertypes(
    ir_name: &str,
    class: &str,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Vec<String> {
    if !carries_surface(ir_name, ir) {
        return Vec::new();
    }
    let s = surface(ir_name, ir, config);
    let mut out = Vec::new();
    // Only a TOTAL order becomes `Comparable`: `_partialCmp` can answer
    // "unordered", which `compareTo` has no way to express (see
    // `emit_partial_compare_to`).
    if s.cmp.is_some() {
        out.push(format!("Comparable<{}>", class));
    }
    if free_fn(&s, ir_name, ir).is_some() {
        out.push("AutoCloseable".to_string());
    }
    out
}

// ============================================================================
// Members
// ============================================================================

/// Emit the whole value surface of `ir_name` into the body of the FFI class
/// `class`. Call it with the builder indented inside the class and BEFORE
/// the nested `ByValue` / `ByReference` declarations — those open a class of
/// their own, and `equals`/`hashCode` have to stay in one body to be read as
/// a pair.
pub(super) fn emit_value_surface(
    builder: &mut CodeBuilder,
    ir_name: &str,
    class: &str,
    ir: &CodegenIR,
    config: &CodegenConfig,
) {
    if !carries_surface(ir_name, ir) {
        return;
    }
    let s = surface(ir_name, ir, config);
    if s.is_empty() {
        return;
    }
    builder.blank();

    if let Some(f) = s.debug {
        emit_to_string(builder, f, ir);
    }
    // equals and hashCode are emitted TOGETHER or not at all: a class whose
    // equality compares VALUES (the C `_partialEq`) and whose hash is the
    // inherited handle hash silently breaks every HashMap and HashSet it is
    // put into.
    if let Some(f) = s.eq {
        emit_equals(builder, class, f, ir);
        emit_hash_code(builder, s.hash, ir);
    } else if s.hash.is_some() {
        // Hash without a value equality: the inherited `equals` is finer
        // than the hash (identical handles are identical values), so the
        // contract still holds.
        emit_hash_code(builder, s.hash, ir);
    }
    if let Some(f) = s.cmp {
        emit_compare_to(builder, class, f, ir);
    }
    if let Some(f) = s.partial_cmp {
        emit_partial_compare_to(builder, class, f, ir);
    }
    if let Some(f) = s.deep_copy {
        emit_deep_copy(builder, f, ir);
    }
    if let Some(f) = free_fn(&s, ir_name, ir) {
        emit_close(builder, f, ir);
    }
    emit_companion(builder, class, &s, ir);
}

/// `override fun toString()` through `Az<T>_toDbgString`.
///
/// Every member below starts with `write()`: JNA keeps a Structure's field
/// values in JVM fields and only syncs them into the native block on write,
/// so the C side must not be handed `pointer` before that sync. (It is a
/// no-op byte-wise for a value the engine just handed back, which JNA has
/// already auto-read, and on a `Union` it writes the active variant alone.)
fn emit_to_string(builder: &mut CodeBuilder, f: &FunctionDef, ir: &CodegenIR) {
    builder.line(&format!("/** Debug text routed through {}. */", f.c_name));
    builder.line("override fun toString(): kotlin.String {");
    builder.indent();
    builder.line("write()");
    builder.line(&format!(
        "return {}({}(pointer))",
        STRING_DECODER,
        call_target(f, ir)
    ));
    builder.dedent();
    builder.line("}");
    builder.blank();
}

fn emit_equals(builder: &mut CodeBuilder, class: &str, f: &FunctionDef, ir: &CodegenIR) {
    // The C ABI answers `bool`, which the JVM type map carries as a `Byte`
    // (JNA would marshal a Java `boolean` as a 4-byte Windows BOOL). Read
    // the declared return rather than assuming either spelling.
    let truthy = if return_kt(f, ir) == "Boolean" {
        ""
    } else {
        ".toInt() != 0"
    };
    builder.line(&format!(
        "/** Value equality routed through {}. */",
        f.c_name
    ));
    builder.line("override fun equals(other: Any?): Boolean {");
    builder.indent();
    builder.line(&format!("if (other !is {}) return false", class));
    builder.line("write()");
    builder.line("other.write()");
    builder.line(&format!(
        "return {}(pointer, other.pointer){}",
        call_target(f, ir),
        truthy
    ));
    builder.dedent();
    builder.line("}");
    builder.blank();
}

fn emit_hash_code(builder: &mut CodeBuilder, f: Option<&FunctionDef>, ir: &CodegenIR) {
    let Some(f) = f else {
        // Equality compares values and the type declares no `Hash`: the only
        // hash that keeps "equal values hash equal" is a constant one.
        builder
            .line("/** Constant: equal values must hash equal, and the type has no value hash. */");
        builder.line("override fun hashCode(): Int = 0");
        builder.blank();
        return;
    };
    builder.line(&format!("/** Value hash routed through {}. */", f.c_name));
    builder.line("override fun hashCode(): Int {");
    builder.indent();
    builder.line("write()");
    builder.line(&format!("val __h = {}(pointer)", call_target(f, ir)));
    if return_kt(f, ir) == "Long" {
        // Fold the engine's 64-bit hash into the JVM's 32-bit one the way
        // `Long.hashCode` does, so both halves contribute.
        builder.line("return (__h xor (__h ushr 32)).toInt()");
    } else {
        builder.line("return __h.toInt()");
    }
    builder.dedent();
    builder.line("}");
    builder.blank();
}

/// `Comparable.compareTo` through `Az<T>_cmp`.
///
/// ORDERING ENCODING (kept in lockstep with `lang_rust`, which writes the
/// other side of the ABI): `0 = Less`, `1 = Equal`, `2 = Greater`. Kotlin
/// wants negative / zero / positive, so "the byte minus one" is exact
/// rather than invented.
fn emit_compare_to(builder: &mut CodeBuilder, class: &str, f: &FunctionDef, ir: &CodegenIR) {
    builder.line(&format!(
        "/** Total order routed through {} (ABI: 0 = Less, 1 = Equal, 2 = Greater). */",
        f.c_name
    ));
    builder.line(&format!("override fun compareTo(other: {}): Int {{", class));
    builder.indent();
    builder.line("write()");
    builder.line("other.write()");
    builder.line(&format!(
        "return {}(pointer, other.pointer).toInt() - 1",
        call_target(f, ir)
    ));
    builder.dedent();
    builder.line("}");
    builder.blank();
}

/// `Az<T>_partialCmp` as a nullable comparison.
///
/// The same ordering byte as `_cmp` plus `255 = unordered`, which no
/// `Comparable` can express — hence a method of its own that answers `null`
/// there instead of inventing an order.
fn emit_partial_compare_to(
    builder: &mut CodeBuilder,
    class: &str,
    f: &FunctionDef,
    ir: &CodegenIR,
) {
    builder.line(&format!(
        "/** Partial order routed through {}; null when the two are unordered. */",
        f.c_name
    ));
    builder.line(&format!("fun partialCompareTo(other: {}): Int? {{", class));
    builder.indent();
    builder.line("write()");
    builder.line("other.write()");
    builder.line(&format!(
        "val __o = {}(pointer, other.pointer).toInt() and 0xFF",
        call_target(f, ir)
    ));
    builder.line("return if (__o > 2) null else __o - 1");
    builder.dedent();
    builder.line("}");
    builder.blank();
}

fn emit_deep_copy(builder: &mut CodeBuilder, f: &FunctionDef, ir: &CodegenIR) {
    builder.line(&format!(
        "/** Deep copy routed through {}: the copy owns its own allocations. */",
        f.c_name
    ));
    builder.line(&format!("fun deepCopy(): {} {{", return_kt(f, ir)));
    builder.indent();
    builder.line("write()");
    builder.line(&format!("return {}(pointer)", call_target(f, ir)));
    builder.dedent();
    builder.line("}");
    builder.blank();
}

/// `AutoCloseable.close()` through `Az<T>_delete`, so `use { }` works.
///
/// Same contract as the wrapper classes' `close()`: idempotent, via a
/// one-shot guard, so the value is freed at most once however often it is
/// closed. There is deliberately NO `Cleaner` registration here — a JNA
/// `Structure`'s block is `Memory` the JVM itself frees at GC time, and a
/// cleaning action calling `_delete` on it would be exactly the double free
/// the guard exists to prevent.
fn emit_close(builder: &mut CodeBuilder, f: &FunctionDef, ir: &CodegenIR) {
    // Private, so JNA's field walk (public non-static fields only) never
    // sees it and the declared field order stays the C layout.
    builder.line("private var __deleted: Boolean = false");
    builder.blank();
    builder.line(&format!(
        "/** Frees the value's native allocations through {}. Idempotent. */",
        f.c_name
    ));
    builder.line("override fun close() {");
    builder.indent();
    builder.line("if (__deleted) return");
    builder.line("__deleted = true");
    builder.line("write()");
    builder.line(&format!("{}(pointer)", call_target(f, ir)));
    builder.dedent();
    builder.line("}");
    builder.blank();
}

/// The static half of the surface: `default()` plus every constructor,
/// static method and instance method api.json declares for the type.
///
/// They are companion functions rather than members because the class's
/// member namespace is not ours: it belongs to JNA's `Structure` / `Union`
/// base (`size()`, `read()`, `write()`, `clear()`, `setType()`, …). A future
/// api.json method named like one of those would either fail to compile or —
/// worse — silently override it. A companion inherits from `Any` alone, and
/// the three names that live there are already renamed by
/// `idiomatic_method_name`.
///
/// Each function is a typed pass-through of the `external fun` beneath it:
/// same parameter types, same return type, so it compiles by construction
/// and adds no marshalling of its own. The one convenience is the receiver
/// of an instance method, which is taken as the value (and synced with
/// `write()`) instead of as a raw `Pointer`.
fn emit_companion(builder: &mut CodeBuilder, class: &str, s: &Surface<'_>, ir: &CodegenIR) {
    if s.default.is_none() && s.api_fns.is_empty() {
        return;
    }
    builder.line("companion object {");
    builder.indent();

    // Two api.json entries can share a Kotlin name (a `Default` derive and a
    // constructor spelled `default`, say). Overloads that differ in their
    // parameters are legal Kotlin and are kept; an exact repeat is dropped.
    let mut seen: BTreeSet<String> = BTreeSet::new();

    if let Some(f) = s.default {
        seen.insert("default()".to_string());
        builder.line(&format!(
            "/** The engine's default value, routed through {}. */",
            f.c_name
        ));
        builder.line(&format!(
            "@JvmStatic fun default(): {} = {}()",
            return_kt(f, ir),
            call_target(f, ir)
        ));
        builder.blank();
    }

    for f in s.api_fns.iter().copied() {
        emit_passthrough(builder, class, f, ir, &mut seen);
    }

    builder.dedent();
    builder.line("}");
    builder.blank();
}

fn emit_passthrough(
    builder: &mut CodeBuilder,
    class: &str,
    f: &FunctionDef,
    ir: &CodegenIR,
    seen: &mut BTreeSet<String>,
) {
    // The same mapping `emit_native_method` used for the declaration, so the
    // call can never disagree with it.
    let declared = |a: &FunctionArg| match a.ref_kind {
        ArgRefKind::Owned => map_kt_owned(&a.type_name, ir),
        ArgRefKind::Ref | ArgRefKind::RefMut | ArgRefKind::Ptr | ArgRefKind::PtrMut => {
            "Pointer?".to_string()
        }
    };
    // A borrowed receiver crosses the C ABI as a bare pointer; at this level
    // it reads as the value itself, whose fields are synced into the native
    // block before that pointer leaves Kotlin.
    let borrowed_receiver = f
        .args
        .first()
        .is_some_and(|a| f.is_receiver_arg(a) && !matches!(a.ref_kind, ArgRefKind::Owned));

    let names: Vec<String> = f
        .args
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let raw = a.name.trim();
            if raw.is_empty() {
                format!("arg{}", i)
            } else {
                sanitize_kt_identifier(raw)
            }
        })
        .collect();
    let types: Vec<String> = f
        .args
        .iter()
        .enumerate()
        .map(|(i, a)| {
            if borrowed_receiver && i == 0 {
                class.to_string()
            } else {
                declared(a)
            }
        })
        .collect();

    let name = idiomatic_method_name(&f.method_name);
    let key = format!("{}({})", name, types.join(","));
    if !seen.insert(key) {
        return;
    }

    let params: Vec<String> = names
        .iter()
        .zip(types.iter())
        .map(|(n, t)| format!("{}: {}", n, t))
        .collect();
    let call_args: Vec<String> = names
        .iter()
        .enumerate()
        .map(|(i, n)| {
            if borrowed_receiver && i == 0 {
                format!("{}.pointer", n)
            } else {
                n.clone()
            }
        })
        .collect();
    let ret = return_kt(f, ir);
    let call = format!("{}({})", call_target(f, ir), call_args.join(", "));

    for d in &f.doc {
        builder.line(&format!("/// {}", kdoc_escape(d)));
    }
    let signature = format!("@JvmStatic fun {}({}): {}", name, params.join(", "), ret);
    if borrowed_receiver {
        builder.line(&format!("{} {{", signature));
        builder.indent();
        builder.line(&format!("{}.write()", names[0]));
        if ret == "Unit" {
            builder.line(&call);
        } else {
            builder.line(&format!("return {}", call));
        }
        builder.dedent();
        builder.line("}");
    } else {
        builder.line(&format!("{} = {}", signature, call));
    }
    builder.blank();
}

// ============================================================================
// Shared AzString decode
// ============================================================================

/// The one type the JVM maps natively: the struct api.json marks as the
/// engine's string. Found by IR category so the emitter never spells the
/// type's name.
fn string_type(ir: &CodegenIR) -> Option<&StructDef> {
    ir.structs
        .iter()
        .find(|s| matches!(s.category, TypeCategory::String))
}

/// The string type's own `_delete`: the returned debug string is owned by
/// the caller, so decoding one without freeing it would leak on every call.
/// `should_emit_function` gates it for the same reason it gates every other
/// call: a config that filters the string type out leaves nothing to call.
fn string_delete<'a>(ir: &'a CodegenIR, config: &CodegenConfig) -> Option<&'a FunctionDef> {
    let s = string_type(ir)?;
    let del = ir
        .functions
        .iter()
        .find(|f| f.class_name == s.name && f.kind == FunctionKind::Delete)?;
    should_emit_function(del, ir, config).then_some(del)
}

fn has_string_decoder(ir: &CodegenIR, config: &CodegenConfig) -> bool {
    string_delete(ir, config).is_some()
}

/// Emit the file-private `azStringIntoKotlin` helper the `toString()`
/// overrides funnel through.
///
/// The wrapper classes inline these same reads for their own `toString()`;
/// the derive surface covers roughly a thousand more types, so it calls one
/// function instead of repeating eight lines a thousand times.
pub(super) fn emit_string_decoder(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) {
    let Some(del) = string_delete(ir, config) else {
        return;
    };
    let Some(s) = string_type(ir) else { return };
    let by_value = map_kt_return(&s.name, ir);

    builder.line("/**");
    builder.line(" * Decode an owned engine string the C ABI just returned into a");
    builder.line(" * `kotlin.String`, then free it.");
    builder.line(" *");
    builder.line(" * The string's layout is `{ vec: <u8 vec> }` and a vec is");
    builder.line(" * `{ ptr, len, cap, destructor }`, so offset 0 is the UTF-8 buffer and");
    builder.line(" * offset 8 its byte length — the same two reads the wrapper classes do");
    builder.line(" * inline. The returned value is owned by the caller, hence the free:");
    builder.line(" * every call site hands this a fresh return value and keeps only the");
    builder.line(" * decoded `kotlin.String`.");
    builder.line(" */");
    builder.line(&format!(
        "private fun {}(__s: {}): kotlin.String {{",
        STRING_DECODER, by_value
    ));
    builder.indent();
    builder.line("__s.write()");
    builder.line("val __sp = __s.pointer");
    builder.line("val __vecPtr: Pointer? = __sp.getPointer(0)");
    builder.line("val __vecLen: Long = __sp.getLong(8)");
    // `ByteArray.toString(Charset)` rather than the `kotlin.String(bytes,
    // charset)` constructor: inside `package com.azul` the bare `String`
    // name can resolve to the emitted wrapper class.
    builder.line("val __out: kotlin.String = if (__vecPtr == null || __vecLen <= 0) \"\" else");
    builder.indent();
    builder.line("__vecPtr.getByteArray(0, __vecLen.toInt()).toString(Charsets.UTF_8)");
    builder.dedent();
    builder.line(&format!("{}(__sp)", call_target(del, ir)));
    builder.line("return __out");
    builder.dedent();
    builder.line("}");
    builder.blank();
}

// ============================================================================
// api.json constants
// ============================================================================

/// Emit every `ir.constants` entry as a Kotlin compile-time constant.
///
/// api.json spells them `<Class>_<NAME>` (`GlContextPtr_ACCUM_ALPHA_BITS`),
/// so they group naturally into one `object <Class>Constants` per declaring
/// class and read as `GlContextPtrConstants.ACCUM_ALPHA_BITS`. A `const val`
/// of a primitive is inlined by the Kotlin compiler into the field's
/// `ConstantValue` attribute, so even ~1400 of them add no class-initialiser
/// bytecode to hit the JVM's 64 KB `<clinit>` limit with.
pub(super) fn emit_constants(builder: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    let mut by_class: BTreeMap<&str, Vec<&ConstantDef>> = BTreeMap::new();
    for c in &ir.constants {
        let Some((class, _)) = c.name.split_once('_') else {
            continue;
        };
        if !config.should_include_type(class) {
            continue;
        }
        by_class.entry(class).or_default().push(c);
    }
    if by_class.is_empty() {
        return;
    }

    builder.line("// ────────────────────────────────────────────────────────────────");
    builder.line("// api.json constants, one object per declaring class.");
    builder.line("//");
    builder.line("// The C ABI's unsigned widths have no JVM equivalent, so each value");
    builder.line("// carries the signed Kotlin type of the same WIDTH (u8 -> Byte,");
    builder.line("// u32 -> Int, u64 -> Long) — the same mapping the FFI declarations");
    builder.line("// above use, so a constant can be passed straight to the call that");
    builder.line("// wants it. The handful of values whose top bit is set are spelled as");
    builder.line("// the negative literal with the same bit pattern.");
    builder.line("// ────────────────────────────────────────────────────────────────");
    builder.blank();

    for (class, consts) in by_class {
        builder.line(&format!(
            "/// The constants the `{}` class declares in api.json.",
            class
        ));
        builder.line(&format!(
            "object {}Constants {{",
            kotlin_class_name(class, ir)
        ));
        builder.indent();
        for c in consts {
            let bare = c.member_name();
            for d in &c.doc {
                builder.line(&format!("/** {} */", kdoc_escape(d)));
            }
            let (kt_type, literal, note) = kt_constant(&c.type_name, &c.value, ir);
            builder.line(&format!(
                "const val {}: {} = {}{}",
                sanitize_kt_identifier(&bare),
                kt_type,
                literal,
                note
            ));
        }
        builder.dedent();
        builder.line("}");
        builder.blank();
    }
}

/// `(Kotlin type, literal, trailing comment)` for one api.json constant.
///
/// The value keeps api.json's own spelling (`0x0D5B`) whenever it fits the
/// signed JVM type; when it does not — `0xFFFFFFFF` as an `Int` — it is
/// re-spelled as the negative literal with the same bits and the original is
/// kept in a comment, because the alternative (widening the type) would stop
/// matching the parameter the constant is passed to.
fn kt_constant(type_name: &str, value: &str, ir: &CodegenIR) -> (String, String, String) {
    let kt = jvm_to_kotlin_primitive(&base_map_jvm_type(type_name, ir));
    let parsed = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .and_then(|hex| u128::from_str_radix(hex, 16).ok())
        .or_else(|| value.parse::<u128>().ok());
    let Some(v) = parsed else {
        // Not a plain integer literal: pass api.json's text through.
        return (kt, value.to_string(), String::new());
    };
    let wrapped = |signed: String| (kt.clone(), signed, format!(" // {}", value));
    match kt.as_str() {
        "Byte" if v > i8::MAX as u128 => wrapped(((v as u8) as i8).to_string()),
        "Short" if v > i16::MAX as u128 => wrapped(((v as u16) as i16).to_string()),
        "Int" if v > i32::MAX as u128 => wrapped(((v as u32) as i32).to_string()),
        "Long" if v > i64::MAX as u128 => wrapped(format!("{}L", (v as u64) as i64)),
        "Long" => (kt.clone(), format!("{}L", value), String::new()),
        _ => (kt.clone(), value.to_string(), String::new()),
    }
}
