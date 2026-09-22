//! Kotlin-idiomatic wrapper-class emission.
//!
//! For every IR struct with a wrapper class (`managed_lang_helpers::
//! has_wrapper_class`: a `_delete` or at least one method) we emit:
//!
//! ```kotlin
//! class App private constructor(private val ptr: Pointer) : AutoCloseable {
//!     companion object {
//!         fun create(...): App = App(AzulNative.AzApp_create(...).pointer!!)
//!     }
//!     override fun close() = AzulNative.AzApp_delete(ptr)
//!     fun run(...) = AzulNative.AzApp_run(ptr, ...)
//! }
//! ```
//!
//! Kotlin's stdlib already provides `AutoCloseable.use { }` so call
//! sites get `App.create(...).use { app -> app.run(...) }` for free.
//! Types without a `_delete` (`CallbackInfo`, POD value types) get the
//! same class shape minus `AutoCloseable` and the GC safety net.
//!
//! Tagged-union enums get a tiny helper `object` with static factory
//! methods per unit variant; payload-bearing variants are left to the
//! user (`Az<Foo>` Union + `Az<Foo>Variant_<Variant>` payload struct).

use anyhow::Result;

use super::{
    super::{
        config::CodegenConfig,
        generator::CodeBuilder,
        ir::{
            ArgRefKind, CallbackTypedefDef, CodegenIR, EnumDef, EnumVariantKind, FieldRefKind,
            FunctionArg, FunctionDef, FunctionKind, MonomorphizedKind, StructDef, TypeCategory,
        },
        lang_java::{functions::native_class_for_class, snake_to_lower_camel},
        managed_host_invoker::{
            app_factory_info, layout_callback_factory_info, smart_callback_setter_info,
            wrapper_name, AppFactoryInfo, LayoutCallbackFactoryInfo,
        },
        managed_lang_helpers::{
            has_delete_function, has_wrapper_class, is_refany_type, takes_self,
        },
    },
    ffi_type_name, kotlin_class_name,
    managed::{kt_data_typed_sam_shape, lower_first},
    map_kt_owned, map_kt_return, sanitize_kt_identifier,
};

/// Phase I.5.1 (Kotlin): how the wrapper method should idiomise an
/// `Option<T>` / `Result<T, E>` return. Mirrors the Java
/// `ReturnIdiom` enum (lang_java/wrappers.rs) — the carried
/// `payload_ty` + `ref_kind` let the emitter compute a typed Kotlin
/// nullable / bare-Ok signature at the wrapper boundary.
#[derive(Clone)]
enum ReturnIdiom {
    Plain,
    Option {
        payload_ty: String,
        ref_kind: FieldRefKind,
    },
    Result {
        payload_ty: String,
        ref_kind: FieldRefKind,
    },
}

/// Detect Az*Option / Az*Result returns via variant shape rather than
/// name prefix. Identical predicate to the Java mirror.
fn classify_return(func: &FunctionDef, ir: &CodegenIR) -> ReturnIdiom {
    let Some(rt) = func.return_type.as_deref() else {
        return ReturnIdiom::Plain;
    };
    let rt = rt.trim();
    if let Some(ta) = ir.find_type_alias(rt) {
        if let Some(ref mono) = ta.monomorphized_def {
            if let MonomorphizedKind::TaggedUnion { ref variants, .. } = mono.kind {
                if variants.len() == 2 {
                    let none = variants.iter().find(|v| v.name == "None");
                    let some = variants.iter().find(|v| v.name == "Some");
                    if let (Some(_), Some(sv)) = (none, some) {
                        if let Some(ref pt) = sv.payload_type {
                            return ReturnIdiom::Option {
                                payload_ty: pt.clone(),
                                ref_kind: sv.payload_ref_kind,
                            };
                        }
                    }
                    let ok = variants.iter().find(|v| v.name == "Ok");
                    let err = variants.iter().find(|v| v.name == "Err");
                    if let (Some(ov), Some(_)) = (ok, err) {
                        if let Some(ref pt) = ov.payload_type {
                            return ReturnIdiom::Result {
                                payload_ty: pt.clone(),
                                ref_kind: ov.payload_ref_kind,
                            };
                        }
                    }
                }
            }
        }
    }
    if let Some(e) = ir.find_enum(rt) {
        if e.variants.len() == 2 {
            let none = e.variants.iter().find(|v| v.name == "None");
            let some = e.variants.iter().find(|v| v.name == "Some");
            if let (Some(_), Some(sv)) = (none, some) {
                if let EnumVariantKind::Tuple(types) = &sv.kind {
                    if types.len() == 1 {
                        return ReturnIdiom::Option {
                            payload_ty: types[0].0.clone(),
                            ref_kind: types[0].1,
                        };
                    }
                }
            }
            let ok = e.variants.iter().find(|v| v.name == "Ok");
            let err = e.variants.iter().find(|v| v.name == "Err");
            if let (Some(ov), Some(_)) = (ok, err) {
                if let EnumVariantKind::Tuple(types) = &ov.kind {
                    if types.len() == 1 {
                        return ReturnIdiom::Result {
                            payload_ty: types[0].0.clone(),
                            ref_kind: types[0].1,
                        };
                    }
                }
            }
        }
    }
    ReturnIdiom::Plain
}

/// Map a payload's "raw" Kotlin field type to the user-visible display
/// type at the wrapper boundary. Same three-case table as Java:
/// `AzString` → `kotlin.String`, `AzX` with wrapper → `X`, otherwise
/// the raw type.
fn payload_display_kt(raw: &str, ir: &CodegenIR) -> String {
    if let Some(unprefixed) = raw.strip_prefix("Az") {
        if let Some(s) = ir.find_struct(unprefixed) {
            if matches!(s.category, TypeCategory::String) {
                return "kotlin.String".to_string();
            }
        }
        if has_wrapper_class(unprefixed, ir) {
            return kotlin_class_name(unprefixed, ir);
        }
    }
    raw.to_string()
}

fn is_az_string_kt(raw: &str, ir: &CodegenIR) -> bool {
    let Some(unprefixed) = raw.strip_prefix("Az") else {
        return false;
    };
    ir.find_struct(unprefixed)
        .map(|s| matches!(s.category, TypeCategory::String))
        .unwrap_or(false)
}

/// Build an `<NativeClass>.Az<OptionT>_delete(__ret.getPointer())`
/// call (or None when there's no _delete export).
fn format_option_delete_call_kt(option_type_name: &str, ir: &CodegenIR) -> Option<String> {
    if !has_delete_function(option_type_name, ir) {
        return None;
    }
    let native = native_class_for_class(option_type_name, ir);
    let ffi_name = ffi_type_name(option_type_name);
    Some(format!(
        "{}.{}_delete(__ret.getPointer())",
        native, ffi_name
    ))
}

/// Build an `<NativeClass>.Az<T>_clone` expression for a
/// wrapper-class payload type, or None if no _clone export exists.
fn format_clone_call_kt(payload_type_name: &str, ir: &CodegenIR) -> Option<String> {
    use super::super::ir::FunctionKind;
    let has_clone = ir
        .functions
        .iter()
        .any(|f| f.class_name == payload_type_name && matches!(f.kind, FunctionKind::DeepCopy));
    if !has_clone {
        return None;
    }
    let native = native_class_for_class(payload_type_name, ir);
    let ffi_name = ffi_type_name(payload_type_name);
    Some(format!("{}.{}_clone", native, ffi_name))
}

/// Emit the body for an `Option<T>` return. `__ret` (the FFI `Az*Option`
/// ByValue) has already been declared.
fn emit_kt_option_body(
    builder: &mut CodeBuilder,
    raw_payload_kt: &str,
    option_type_name: &str,
    ir: &CodegenIR,
) {
    let option_delete = format_option_delete_call_kt(option_type_name, ir);
    let emit_delete = |b: &mut CodeBuilder| {
        if let Some(ref del) = option_delete {
            b.line(del);
        }
    };
    if is_az_string_kt(raw_payload_kt, ir) {
        builder.line("val __nv = __ret.toNullable()");
        builder.line("if (__nv == null) {");
        builder.indent();
        emit_delete(builder);
        builder.line("return null");
        builder.dedent();
        builder.line("}");
        builder.line("val __sp = __nv.pointer");
        builder.line("val __vp = __sp.getPointer(0)");
        builder.line("val __vl = __sp.getLong(8)");
        builder.line("val __out: kotlin.String = if (__vp == null || __vl <= 0) \"\" else");
        builder.indent();
        builder.line("__vp.getByteArray(0, __vl.toInt()).toString(Charsets.UTF_8)");
        builder.dedent();
        emit_delete(builder);
        builder.line("return __out");
        return;
    }
    if let Some(unprefixed) = raw_payload_kt.strip_prefix("Az") {
        if has_wrapper_class(unprefixed, ir) {
            let wrapper = kotlin_class_name(unprefixed, ir);
            let clone_call = format_clone_call_kt(unprefixed, ir);
            builder.line("val __nv = __ret.toNullable()");
            builder.line("if (__nv == null) {");
            builder.indent();
            emit_delete(builder);
            builder.line("return null");
            builder.dedent();
            builder.line("}");
            if let Some(ref clone) = clone_call {
                builder.line(&format!(
                    "val __cloned = {}(__nv.pointer) as {}.ByValue",
                    clone, raw_payload_kt
                ));
                builder.line("__cloned.write()");
                emit_delete(builder);
                builder.line(&format!("return {}(__cloned.pointer)", wrapper));
            } else {
                builder.line(&format!("return {}(__nv.pointer)", wrapper));
            }
            return;
        }
    }
    // Primitive / Pointer payload — independent value already.
    builder.line("val __opt = __ret.toNullable()");
    emit_delete(builder);
    builder.line("return __opt");
}

/// Emit the body for a `Result<T, E>` return (throws on Err — same
/// idiom as Rust's `Result::unwrap`).
fn emit_kt_result_body(
    builder: &mut CodeBuilder,
    raw_payload_kt: &str,
    result_type_name: &str,
    ir: &CodegenIR,
) {
    let result_delete = format_option_delete_call_kt(result_type_name, ir);
    let emit_delete = |b: &mut CodeBuilder| {
        if let Some(ref del) = result_delete {
            b.line(del);
        }
    };
    if is_az_string_kt(raw_payload_kt, ir) {
        builder.line("val __u = __ret.unwrap()");
        builder.line("val __sp = __u.pointer");
        builder.line("val __vp = __sp.getPointer(0)");
        builder.line("val __vl = __sp.getLong(8)");
        builder.line("val __out: kotlin.String = if (__vp == null || __vl <= 0) \"\" else");
        builder.indent();
        builder.line("__vp.getByteArray(0, __vl.toInt()).toString(Charsets.UTF_8)");
        builder.dedent();
        emit_delete(builder);
        builder.line("return __out");
        return;
    }
    if let Some(unprefixed) = raw_payload_kt.strip_prefix("Az") {
        if has_wrapper_class(unprefixed, ir) {
            let wrapper = kotlin_class_name(unprefixed, ir);
            let clone_call = format_clone_call_kt(unprefixed, ir);
            builder.line("val __u = __ret.unwrap()");
            if let Some(ref clone) = clone_call {
                builder.line(&format!(
                    "val __cloned = {}(__u.pointer) as {}.ByValue",
                    clone, raw_payload_kt
                ));
                builder.line("__cloned.write()");
                emit_delete(builder);
                builder.line(&format!("return {}(__cloned.pointer)", wrapper));
            } else {
                builder.line(&format!("return {}(__u.pointer)", wrapper));
            }
            return;
        }
    }
    builder.line("val __u = __ret.unwrap()");
    emit_delete(builder);
    builder.line("return __u");
}

pub fn emit_all(builder: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) -> Result<()> {
    builder.line("// --------------------------------------------------------------------------");
    builder.line("// Idiomatic Kotlin wrappers (AutoCloseable + companion-object factories).");
    builder.line("// --------------------------------------------------------------------------");
    builder.blank();

    // GC safety net shared by every wrapper. Unlike Java (which uses a
    // `finalize()` override), Kotlin/modern JVM leans on
    // `java.lang.ref.Cleaner`: one shared Cleaner spawns a single
    // background thread that, if the user forgets `close()`/`use { }`,
    // frees the still-owned native pointer at GC time. Each wrapper
    // registers a cleaning action that references ONLY the raw pointer +
    // a one-shot guard (never `this`, which would keep the wrapper
    // reachable forever and defeat the Cleaner). `close()`/`__consume()`
    // pre-empt the Cleaner explicitly so a pointer is freed at most once.
    // File-private: visible to every wrapper class emitted below.
    builder
        .line("private val AZUL_CLEANER: java.lang.ref.Cleaner = java.lang.ref.Cleaner.create()");
    builder.blank();

    // The application-object pattern (`App.create(model, ::layout)` +
    // `app.run(options)`) is matched structurally once for the whole IR.
    let app_info = app_factory_info(ir);

    for s in &ir.structs {
        if !should_emit_wrapper(s, ir, config) {
            continue;
        }
        emit_wrapper(builder, s, ir, app_info.as_ref());
    }

    for e in &ir.enums {
        if !should_emit_helper(e, config) {
            continue;
        }
        emit_union_helper(builder, e, ir);
    }

    Ok(())
}

/// One predicate for "this struct gets a wrapper class", shared with the
/// typed-SAM bridge and the arg/return converters (`has_wrapper_class`).
fn should_emit_wrapper(s: &StructDef, ir: &CodegenIR, config: &CodegenConfig) -> bool {
    config.should_include_type(&s.name) && has_wrapper_class(&s.name, ir)
}

fn should_emit_helper(e: &EnumDef, config: &CodegenConfig) -> bool {
    if !config.should_include_type(&e.name) {
        return false;
    }
    if !e.generic_params.is_empty() {
        return false;
    }
    if matches!(
        e.category,
        TypeCategory::Recursive | TypeCategory::GenericTemplate | TypeCategory::DestructorOrClone
    ) {
        return false;
    }
    e.is_union
}

/// Phase I.1.3 (Kotlin): Vec-shape detector. Same predicate as Haskell
/// H.3 / Ruby I.1.6 / Java I.1.2.
fn detect_vec_elem_type_kt(s: &StructDef) -> Option<String> {
    if s.fields.len() != 4 {
        return None;
    }
    if s.fields[0].name != "ptr" || s.fields[1].name != "len" || s.fields[2].name != "cap" {
        return None;
    }
    if s.fields[1].type_name.trim() != "usize" || s.fields[2].type_name.trim() != "usize" {
        return None;
    }
    let raw = s.fields[0].type_name.trim();
    let elem = raw
        .strip_prefix("*mut ")
        .or_else(|| raw.strip_prefix("*const "))
        .map(str::trim)
        .unwrap_or(raw);
    if elem.is_empty() {
        return None;
    }
    Some(elem.to_string())
}

fn emit_wrapper(
    builder: &mut CodeBuilder,
    s: &StructDef,
    ir: &CodegenIR,
    app_info: Option<&AppFactoryInfo>,
) {
    let has_delete = has_delete_function(&s.name, ir);
    let class_name = kotlin_class_name(&s.name, ir);
    let ffi_name = ffi_type_name(&s.name);

    if !s.doc.is_empty() {
        // (KDoc switched to triple-slash to bypass parser issues with `*/` in inline code samples)
        for d in &s.doc {
            builder.line(&format!("/// {}", kdoc_escape(d)));
        }
    }

    // Phase I.1.3 (Kotlin): when this wrapper's underlying struct is a
    // Vec with a wrapper-class element, declare `Iterable<T>` so
    // `for (x in vec) { ... }` works idiomatically.
    let vec_elem_type = detect_vec_elem_type_kt(s);
    let vec_elem_has_wrapper = |elem: &str| -> bool { has_wrapper_class(elem, ir) };
    let mut supertypes: Vec<String> = Vec::new();
    if has_delete {
        supertypes.push("AutoCloseable".to_string());
    }
    // A total order (api.json `Ord` + the `Az<X>_cmp` export) makes the
    // wrapper `Comparable`, so `a < b`, `sorted()` and `maxOrNull()` all
    // work on it. See `emit_kt_ordering_if_supported` for why a merely
    // partial one does not.
    if ordering_symbol(s, ir, FunctionKind::Cmp).is_some() {
        supertypes.push(format!("Comparable<{}>", class_name));
    }
    if let Some(elem) = vec_elem_type.as_deref().filter(|e| vec_elem_has_wrapper(e)) {
        supertypes.push(format!("Iterable<{}>", kotlin_class_name(elem, ir)));
    }
    let supertypes = if supertypes.is_empty() {
        String::new()
    } else {
        format!(" : {}", supertypes.join(", "))
    };

    // `internal constructor`: sibling wrappers and the codegen-emitted
    // bridges construct wrappers from raw pointers; users go through the
    // static factories. Owning types additionally take `owned`: `false`
    // when the pointee belongs to the engine (callback args, Vec elements
    // without `_clone`) — no GC safety net is registered and neither
    // `close()` nor the Cleaner ever frees it.
    if has_delete {
        builder.line(&format!(
            "class {} internal constructor(internal val ptr: Pointer, owned: Boolean = true){} {{",
            class_name, supertypes
        ));
    } else {
        builder.line(&format!(
            "class {} internal constructor(internal val ptr: Pointer){} {{",
            class_name, supertypes
        ));
    }
    builder.indent();

    builder.line("private var closed: Boolean = false");
    builder.blank();

    // GC safety net (see AZUL_CLEANER above). The cleaning action is
    // built in a `run { }` block that captures LOCAL copies of the
    // pointer + guard — never `this` — so registering it does not keep
    // the wrapper strongly reachable. `__cleanFreed` is a one-shot guard
    // shared with the action: whoever wins the CAS performs (or skips)
    // the free, so the pointer is deleted at most once regardless of
    // close() / __consume() / GC ordering. Only types with a `_delete`
    // have anything to free.
    if has_delete {
        builder.line("private val __cleanFreed = java.util.concurrent.atomic.AtomicBoolean(false)");
        builder.line(
            "private val __cleanable: java.lang.ref.Cleaner.Cleanable? = if (!owned) null else run {",
        );
        builder.indent();
        builder.line("val __p = ptr");
        builder.line("val __guard = __cleanFreed");
        builder.line("AZUL_CLEANER.register(this, Runnable {");
        builder.indent();
        builder.line("if (__guard.compareAndSet(false, true)) {");
        builder.indent();
        builder.line(&format!(
            "{}.{}_delete(__p)",
            native_class_for_class(&s.name, ir),
            ffi_name
        ));
        builder.dedent();
        builder.line("}");
        builder.dedent();
        builder.line("})");
        builder.dedent();
        builder.line("}");
        builder.blank();
    }

    // Internal pointer access for sibling wrappers.
    builder.line("/** Internal: raw pointer for use by sibling wrappers. */");
    builder.line("fun rawPointer(): Pointer = ptr");
    builder.blank();

    let is_app_class = app_info.is_some_and(|i| i.class_name == s.name);
    let window_options_info: Option<&LayoutCallbackFactoryInfo> = app_info.and_then(|i| {
        i.window_methods
            .iter()
            .find(|(_, _, w)| w.class_name == s.name)
            .map(|(_, _, w)| w)
    });
    if let Some(info) = app_info.filter(|_| is_app_class) {
        emit_kt_app_factory_fields(builder, info, ir);
    }

    // AzString gets a `toString()` override that decodes the wrapped
    // UTF-8 bytes into a `kotlin.String`. AzString's C-side layout is
    // `{ vec: AzU8Vec }`, AzU8Vec is `{ ptr, len, cap, destructor }`,
    // so offset 0 is `vec.ptr` (the UTF-8 byte buffer) and offset 8 is
    // `vec.len` (byte length).
    if matches!(s.category, TypeCategory::String) {
        builder.line("/**");
        builder.line(" * Decode the wrapped UTF-8 bytes into a `kotlin.String`.");
        builder.line(" * Reads `vec.ptr` (offset 0) and `vec.len` (offset 8) from");
        builder.line(" * the AzString struct directly via JNA.");
        builder.line(" */");
        builder.line("override fun toString(): kotlin.String {");
        builder.indent();
        builder.line("if (closed) return \"\"");
        builder.line("val vecPtr: Pointer? = ptr.getPointer(0)");
        builder.line("val vecLen: Long = ptr.getLong(8)");
        builder.line("if (vecPtr == null || vecLen <= 0) return \"\"");
        builder.line("val bytes = vecPtr.getByteArray(0, vecLen.toInt())");
        // ByteArray.toString(Charset) — Kotlin's idiom for UTF-8
        // decode. Using `kotlin.String(bytes, charset)` constructor
        // form clashes with the local `String` wrapper class when
        // imports collide.
        builder.line("return bytes.toString(Charsets.UTF_8)");
        builder.dedent();
        builder.line("}");
        builder.blank();
    }

    // Companion object holding the static factories.
    let static_funcs: Vec<&FunctionDef> = ir
        .functions_for_class(&s.name)
        .filter(|f| {
            !f.kind.is_trait_function()
                && matches!(
                    f.kind,
                    FunctionKind::Constructor | FunctionKind::StaticMethod | FunctionKind::Default
                )
        })
        .collect();

    // Smart factories live inside the companion too, so it is opened
    // even when `static_funcs` is empty.
    let layout_factory_info = layout_callback_factory_info(s, ir);
    let needs_companion = !static_funcs.is_empty()
        || layout_factory_info.is_some()
        || is_app_class
        || window_options_info.is_some();
    if needs_companion {
        builder.line("companion object {");
        builder.indent();
        if let Some(info) = layout_factory_info.as_ref() {
            emit_kt_layout_callback_factories(builder, info, ir);
        }
        if let Some(info) = window_options_info {
            emit_kt_default_options_factory(builder, &class_name, info, &static_funcs, ir);
        }
        if let Some(info) = app_info.filter(|_| is_app_class) {
            emit_kt_app_factory_companion(builder, &class_name, info, ir);
        }
        for func in static_funcs {
            emit_static_factory(builder, &class_name, &ffi_name, func, ir);
        }
        builder.dedent();
        builder.line("}");
        builder.blank();
    }

    // Instance methods.
    for func in ir.functions_for_class(&s.name) {
        if func.kind.is_trait_function() {
            continue;
        }
        if matches!(
            func.kind,
            FunctionKind::Constructor | FunctionKind::StaticMethod | FunctionKind::Default
        ) {
            continue;
        }
        emit_instance_method(builder, &class_name, &ffi_name, func, ir, app_info);
    }

    // Phase I.2 (Kotlin): equals + hashCode routed through the
    // codegen-emitted C-ABI helpers when TypeTraits says they exist.
    emit_kt_equals_hashcode_if_supported(builder, s, &class_name, &ffi_name, ir);

    // Phase I.3 (Kotlin): toString() routed through Az<X>_toDbgString.
    emit_kt_to_string_if_supported(builder, s, ir);

    // Ordering routed through Az<X>_cmp / Az<X>_partialCmp.
    emit_kt_ordering_if_supported(builder, s, &class_name, ir);

    // Phase I.1.3 (Kotlin): iterator() body for Vec wrappers with a
    // wrapper-class element type. Mirrors Java's I.1.2 emission via
    // JNA Structure.newInstance. Primitive-element Vecs get a
    // bulk-copy sibling array (`toByteArray()` / `toIntArray()` /
    // …) instead.
    if let Some(elem) = vec_elem_type.as_deref() {
        if vec_elem_has_wrapper(elem) {
            emit_kt_vec_iterator(builder, s, elem, ir);
        } else {
            emit_kt_vec_primitive_array(builder, s, elem);
        }
    }

    if has_delete {
        // close(): drive the free through the Cleaner's `clean()` so the
        // delete path is shared with the GC safety net and runs at most
        // once. `clean()` invokes the cleaning action (which wins the
        // __cleanFreed CAS and calls Az<X>_delete) and deregisters the
        // Cleanable, so a later GC never re-runs it. Idempotent via
        // `closed` + the one-shot guard. A borrowed wrapper (`owned =
        // false`) has no Cleanable and frees nothing.
        builder.line("/** Frees the underlying native resources. Idempotent. */");
        builder.line("override fun close() {");
        builder.indent();
        builder.line("if (closed) return");
        builder.line("closed = true");
        builder.line("__cleanable?.clean()");
        builder.dedent();
        builder.line("}");
        builder.blank();

        // Mark this wrapper as consumed without calling Az<X>_delete.
        // Used by codegen-emitted call sites where the C ABI takes
        // ownership of the underlying bytes by-value. We must cancel the
        // GC safety net WITHOUT freeing: set the one-shot guard first (so
        // the action no-ops), then `clean()` to deregister the Cleanable
        // so the Cleaner thread can never fire a double-free after the C
        // side has taken ownership.
        builder.line(
            "/** Internal: mark consumed (called by codegen-emitted bridges that transfer \
             ownership to the C ABI by-value, or when a borrowed pointee goes out of scope). */",
        );
        builder.line("internal fun __consume() {");
        builder.indent();
        builder.line("if (closed) return");
        builder.line("closed = true");
        builder.line("__cleanFreed.set(true)");
        builder.line("__cleanable?.clean()");
        builder.dedent();
        builder.line("}");
    } else {
        // Nothing to free: the type has no `_delete`. `__consume()` only
        // invalidates the wrapper — its bytes were copied into the engine,
        // or the engine-owned memory it borrowed (a `CallbackInfo` during
        // a callback) is gone.
        builder.line(
            "/** Internal: invalidate this wrapper (engine-owned memory it borrowed went out of \
             scope). Nothing to free. */",
        );
        builder.line("internal fun __consume() {");
        builder.indent();
        builder.line("closed = true");
        builder.dedent();
        builder.line("}");
    }

    builder.dedent();
    builder.line("}");
    builder.blank();
}

/// `<class>.create(fn)` smart factories for a struct matching
/// `layout_callback_factory_info` (today `WindowCreateOptions`): register
/// the SAM via the host invoker and splice the wrapper struct into the
/// nested callback field — preserving the host-handle ctx that the raw
/// `_create(rawFnPtr)` path discards.
fn emit_kt_layout_callback_factories(
    builder: &mut CodeBuilder,
    info: &LayoutCallbackFactoryInfo,
    ir: &CodegenIR,
) {
    let wrapper_class = kotlin_class_name(&info.class_name, ir);
    let register_fn = format!("register{}", info.callback_wrapper);
    let native_class = native_class_for_class(&info.class_name, ir);
    let field_path = info.field_path.join(".");
    let sam_raw = format!("AzulNativeManaged.{}InvokerCallback", info.callback_wrapper);
    let sam_typed = format!("AzulHostInvoker.{}", info.callback_wrapper);

    for (sam_type, doc_note) in [
        (
            sam_raw.as_str(),
            "Smart factory: pass a layout-callback lambda; the host-invoker registration \
             and bytes-copy plumbing happen internally.",
        ),
        (
            sam_typed.as_str(),
            "Smart factory (typed): pass a typed callback that returns a wrapper \
             directly; the bridge splices the bytes into the embedded callback field.",
        ),
    ] {
        builder.line("/**");
        builder.line(&format!(" * {}", doc_note));
        builder.line(" */");
        builder.line(&format!(
            "fun create(fn: {}): {} {{",
            sam_type, wrapper_class
        ));
        builder.indent();
        builder.line(&format!("val __cb = AzulHostInvoker.{}(fn)", register_fn));
        builder.line(&format!(
            "val __wco = {}.{}()",
            native_class, info.default_c_name
        ));
        builder.line("__cb.write()");
        builder.line("__wco.write()");
        builder.line("val __cbBytes = __cb.getPointer().getByteArray(0, __cb.size())");
        builder.line(&format!(
            "__wco.{}.getPointer().write(0, __cbBytes, 0, __cbBytes.size)",
            field_path
        ));
        builder.line("__wco.read()");
        builder.line(&format!("return {}(__wco.getPointer())", wrapper_class));
        builder.dedent();
        builder.line("}");
        builder.blank();
    }
}

/// Zero-arg `create()` for the window-options struct of the application
/// pattern (`WindowCreateOptions.create()`): the engine-default callback
/// stays in place and `App.create(data, fn)` swaps in the registered one
/// when the options reach `run`/`addWindow`. Skipped when the C API
/// already has a zero-arg constructor of that name.
fn emit_kt_default_options_factory(
    builder: &mut CodeBuilder,
    class_name: &str,
    info: &LayoutCallbackFactoryInfo,
    static_funcs: &[&FunctionDef],
    ir: &CodegenIR,
) {
    if static_funcs
        .iter()
        .any(|f| f.args.is_empty() && idiomatic_method_name(&f.method_name) == "create")
    {
        return;
    }
    let native_class = native_class_for_class(&info.class_name, ir);
    builder.line("/**");
    builder.line(&format!(
        " * Options carrying the engine-default `{}`; an application created with",
        info.callback_wrapper
    ));
    builder
        .line(" * `create(data, fn)` substitutes its registered callback when these options are");
    builder.line(" * passed to a window-opening method. Equivalent to `createDefault()`.");
    builder.line(" */");
    builder.line(&format!("@JvmStatic fun create(): {} {{", class_name));
    builder.indent();
    builder.line(&format!(
        "val raw = {}.{}()",
        native_class, info.default_c_name
    ));
    builder.line(&format!("return {}(raw.pointer)", class_name));
    builder.dedent();
    builder.line("}");
    builder.blank();
}

/// Distinct `(options class, factory info)` pairs among an application
/// class's window-taking methods, in first-seen order.
fn app_window_option_types(info: &AppFactoryInfo) -> Vec<&LayoutCallbackFactoryInfo> {
    let mut out: Vec<&LayoutCallbackFactoryInfo> = Vec::new();
    for (_, _, w) in &info.window_methods {
        if !out.iter().any(|o| o.class_name == w.class_name) {
            out.push(w);
        }
    }
    out
}

/// Distinct callback kinds among an application class's window methods.
fn app_callback_kinds(info: &AppFactoryInfo) -> Vec<&str> {
    let mut out: Vec<&str> = Vec::new();
    for (_, _, w) in &info.window_methods {
        if !out.contains(&w.callback_wrapper.as_str()) {
            out.push(&w.callback_wrapper);
        }
    }
    out
}

/// Name of the fn-pointer field inside a callback wrapper struct
/// (`LayoutCallback { cb, ctx }` → `"cb"`): the field whose type is a
/// callback typedef. Derived from the IR, never spelled out.
fn callback_fn_field(kind: &str, ir: &CodegenIR) -> Option<String> {
    let s = ir.find_struct(kind)?;
    s.fields
        .iter()
        .find(|f| {
            let t = f.type_name.trim();
            ir.callback_typedefs.iter().any(|c| c.name == t)
        })
        .map(|f| f.name.clone())
}

/// Instance-level part of the application factory: one registration
/// closure per callback kind (set by `create(data, fn)`) and one splice
/// helper per window-options type. Each window that still carries the
/// engine-default callback gets a FRESH host handle: the engine releases
/// every window's callback independently, so callback bytes (which own a
/// `RefAny` refcount) are never duplicated between windows.
fn emit_kt_app_factory_fields(builder: &mut CodeBuilder, info: &AppFactoryInfo, ir: &CodegenIR) {
    for kind in app_callback_kinds(info) {
        builder.line(&format!(
            "/** Registers a fresh host handle for the `{}` given to `create(data, fn)`; \
             `null` when the application was created with an explicit config. */",
            kind
        ));
        builder.line(&format!(
            "private var __{}Factory: (() -> {}.ByValue)? = null",
            lower_first(kind),
            ffi_type_name(kind)
        ));
        builder.blank();
    }
    for w in app_window_option_types(info) {
        let Some(cb_field) = callback_fn_field(&w.callback_wrapper, ir) else {
            continue;
        };
        builder.line(&format!(
            "/** Internal: when `opts.{}` still holds the engine default, write the callback \
             registered by `create(data, fn)` into it. */",
            w.field_path.join(".")
        ));
        builder.line(&format!(
            "private fun __spliceInto{}(opts: {}) {{",
            w.class_name,
            ffi_type_name(&w.class_name)
        ));
        builder.indent();
        builder.line(&format!(
            "val __make = __{}Factory ?: return",
            lower_first(&w.callback_wrapper)
        ));
        builder.line(&format!("val __slot = opts.{}", w.field_path.join(".")));
        builder.line(&format!(
            "if (__slot.{} != __{}DefaultCb) return",
            cb_field, w.class_name
        ));
        builder.line("val __cb = __make()");
        builder.line("__cb.write()");
        builder.line("val __bytes = __cb.pointer.getByteArray(0, __cb.size())");
        builder.line("__slot.pointer.write(0, __bytes, 0, __bytes.size)");
        builder.line("opts.read()");
        builder.dedent();
        builder.line("}");
        builder.blank();
    }
}

/// Companion part of the application factory: the engine-default fn
/// pointer per window-options type (read once from a fresh default
/// instance) and the guide's typed `create(data, fn)`.
fn emit_kt_app_factory_companion(
    builder: &mut CodeBuilder,
    class_name: &str,
    info: &AppFactoryInfo,
    ir: &CodegenIR,
) {
    for w in app_window_option_types(info) {
        let Some(cb_field) = callback_fn_field(&w.callback_wrapper, ir) else {
            continue;
        };
        let native_w = native_class_for_class(&w.class_name, ir);
        builder.line(&format!(
            "/** The `{}` of the engine-default `{}` inside a fresh `{}()`. */",
            cb_field, w.callback_wrapper, w.default_c_name
        ));
        builder.line(&format!(
            "private val __{}DefaultCb: Pointer? by lazy {{",
            w.class_name
        ));
        builder.indent();
        builder.line(&format!("val __d = {}.{}()", native_w, w.default_c_name));
        builder.line(&format!(
            "val __p = __d.{}.{}",
            w.field_path.join("."),
            cb_field
        ));
        if has_delete_function(&w.class_name, ir) {
            builder.line(&format!(
                "{}.{}_delete(__d.pointer)",
                native_w,
                ffi_type_name(&w.class_name)
            ));
        }
        builder.line("__p");
        builder.dedent();
        builder.line("}");
        builder.blank();
    }

    let native = native_class_for_class(&info.class_name, ir);
    let config_native = native_class_for_class(&info.config_type, ir);
    for kind in app_callback_kinds(info) {
        let Some(cb) = ir
            .callback_typedefs
            .iter()
            .find(|c| wrapper_name(c) == kind)
        else {
            continue;
        };
        if kt_data_typed_sam_shape(cb, ir).is_none() {
            continue;
        }
        let mut create_args = vec![String::new(); 2];
        create_args[info.data_arg_index] = "__data".to_string();
        create_args[info.config_arg_index] = "__config".to_string();
        builder.line("/**");
        builder.line(&format!(
            " * Create the application from a data model and a typed `{}`:",
            kind
        ));
        builder.line(&format!(
            " * `{}.create(model, ::layout)`. The model is wrapped in a `RefAny` host handle, the",
            class_name
        ));
        builder.line(&format!(
            " * config is `{}()`, and every window opened with options that still carry the",
            info.config_default_c_name
        ));
        builder.line(" * engine-default callback receives `fn` (an explicit `create(fn)` on the options wins).");
        builder.line(" */");
        builder.line(&format!(
            "@JvmStatic fun <T : Any> create(data: T, fn: AzulHostInvoker.{}WithData<T>): {} {{",
            kind, class_name
        ));
        builder.indent();
        builder.line("val __klass = data.javaClass");
        builder.line("val __data = AzulHostInvoker.refanyCreate(data)");
        builder.line(&format!(
            "val __config = {}.{}()",
            config_native, info.config_default_c_name
        ));
        builder.line(&format!(
            "val raw = {}.{}({})",
            native,
            info.create_c_name,
            create_args.join(", ")
        ));
        builder.line(&format!("val __app = {}(raw.pointer)", class_name));
        builder.line(&format!(
            "__app.__{}Factory = {{ AzulHostInvoker.register{}(__klass, fn) }}",
            lower_first(kind),
            kind
        ));
        builder.line("return __app");
        builder.dedent();
        builder.line("}");
        builder.blank();
    }
}

/// The Kotlin parameter type of one user-visible wrapper-method arg. Owned
/// `String`s take a `kotlin.String`, owned wrapper-class structs take the
/// wrapper instance, references collapse to `Pointer?`.
fn kt_user_arg_type(a: &FunctionArg, ir: &CodegenIR) -> String {
    if is_az_string_owned_arg(a, ir) {
        "kotlin.String".to_string()
    } else if is_wrapper_class_owned_arg(a, ir) {
        kotlin_class_name(a.type_name.trim(), ir)
    } else {
        match a.ref_kind {
            ArgRefKind::Owned => map_kt_owned(&a.type_name, ir),
            ArgRefKind::Ref | ArgRefKind::RefMut | ArgRefKind::Ptr | ArgRefKind::PtrMut => {
                "Pointer?".to_string()
            }
        }
    }
}

/// Smart callback setters for a `with_on_*(self, data: RefAny, cb: <Kind>)`
/// method (shared detector: `smart_callback_setter_info`), emitted right
/// after the plain method. Two overloads, both forwarding to the plain one:
///
/// * `onClick(data: Any, fn: <Kind>InvokerCallback)` — raw SAM, receives `Pointer`s;
/// * `withOnClick(data: T, fn: <Kind>WithData<T>)` — typed SAM (`fun onClick(data: Counter,
///   info: CallbackInfo): Update` resolves here), only when `kt_data_typed_sam_shape` says the
///   SAM exists for this kind.
///
/// The receiver, data and callback positions come from the arg list by
/// position and IR type, never from C arg names.
fn emit_kt_smart_setters(
    builder: &mut CodeBuilder,
    func: &FunctionDef,
    displayed_return: &str,
    ir: &CodegenIR,
) {
    let Some((smart_snake, kind)) = smart_callback_setter_info(func) else {
        return;
    };
    if !takes_self(func) {
        return;
    }
    let data_idx = func
        .args
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, a)| is_refany_type(&a.type_name, ir))
        .map(|(i, _)| i);
    let cb_idx = func
        .args
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, a)| a.type_name.trim() == kind)
        .map(|(i, _)| i);
    let (Some(data_idx), Some(cb_idx)) = (data_idx, cb_idx) else {
        return;
    };
    let plain_name = idiomatic_method_name(&func.method_name);
    let passthrough: Vec<String> = func
        .args
        .iter()
        .enumerate()
        .skip(1)
        .filter(|(i, _)| *i != data_idx && *i != cb_idx)
        .map(|(_, a)| {
            format!(
                "{}: {}",
                sanitize_kt_identifier(&a.name),
                kt_user_arg_type(a, ir)
            )
        })
        .collect();
    let sig_tail = if passthrough.is_empty() {
        String::new()
    } else {
        format!(", {}", passthrough.join(", "))
    };
    let forward = |data_expr: &str, cb_expr: &str| -> String {
        func.args
            .iter()
            .enumerate()
            .skip(1)
            .map(|(i, a)| {
                if i == data_idx {
                    data_expr.to_string()
                } else if i == cb_idx {
                    cb_expr.to_string()
                } else {
                    sanitize_kt_identifier(&a.name)
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    let ret_kw = if displayed_return == "Unit" {
        ""
    } else {
        "return "
    };
    let data_class = kotlin_class_name(func.args[data_idx].type_name.trim(), ir);
    let cb_class = kotlin_class_name(&kind, ir);

    let smart_camel = snake_to_lower_camel(&smart_snake);
    builder.line("/**");
    builder.line(&format!(
        " * Smart builder for {}: raw SAM + host object. Auto-registers via",
        plain_name
    ));
    builder.line(" * AzulHostInvoker.");
    builder.line(" */");
    builder.line(&format!(
        "fun {}(data: Any, fn: AzulNativeManaged.{}InvokerCallback{}): {} {{",
        smart_camel, kind, sig_tail, displayed_return
    ));
    builder.indent();
    builder.line("val __data = AzulHostInvoker.refanyCreate(data)");
    builder.line(&format!("val __cb = AzulHostInvoker.register{}(fn)", kind));
    builder.line(&format!(
        "{}this.{}({})",
        ret_kw,
        plain_name,
        forward(
            &format!("{}(__data.pointer)", data_class),
            &format!("{}(__cb.pointer)", cb_class)
        )
    ));
    builder.dedent();
    builder.line("}");
    builder.blank();

    let Some(cb) = ir
        .callback_typedefs
        .iter()
        .find(|c| wrapper_name(c) == kind)
    else {
        return;
    };
    if kt_data_typed_sam_shape(cb, ir).is_none() {
        return;
    }
    builder.line("/**");
    builder.line(&format!(
        " * Typed smart builder for {}: `fn` receives the model as `T` and wrapper-class",
        plain_name
    ));
    builder.line(" * args; `T` is inferred from `data`.");
    builder.line(" */");
    builder.line(&format!(
        "fun <T : Any> {}(data: T, fn: AzulHostInvoker.{}WithData<T>{}): {} {{",
        plain_name, kind, sig_tail, displayed_return
    ));
    builder.indent();
    builder.line("val __data = AzulHostInvoker.refanyCreate(data)");
    builder.line(&format!(
        "val __cb = AzulHostInvoker.register{}(data.javaClass, fn)",
        kind
    ));
    builder.line(&format!(
        "{}this.{}({})",
        ret_kw,
        plain_name,
        forward(
            &format!("{}(__data.pointer)", data_class),
            &format!("{}(__cb.pointer)", cb_class)
        )
    ));
    builder.dedent();
    builder.line("}");
    builder.blank();
}

/// Auto-string-conversion rule: any Owned arg of the engine's STRING type
/// accepts a `kotlin.String` at the wrapper level. Returns true if this
/// arg should be re-typed to `kotlin.String` and converted in pre-call
/// lines. Which type that is comes from the IR category (the same one
/// `is_az_string_kt` reads), never from its api.json spelling: renaming
/// the type in api.json must not silently turn every string parameter in
/// this binding back into a raw struct.
fn is_az_string_owned_arg(a: &FunctionArg, ir: &CodegenIR) -> bool {
    matches!(a.ref_kind, ArgRefKind::Owned)
        && ir
            .find_struct(a.type_name.trim())
            .is_some_and(|s| matches!(s.category, TypeCategory::String))
}

/// Auto-wrapper-class rule: any Owned arg whose type has a wrapper class
/// (`has_wrapper_class`, the same predicate that emits the class) accepts
/// the wrapper instance at the param; the pre-call splice writes the
/// underlying Pointer into a `.ByValue` Structure overlay so the C ABI
/// sees a real struct value. Pure type-driven; no method-name allowlist.
fn is_wrapper_class_owned_arg(a: &FunctionArg, ir: &CodegenIR) -> bool {
    matches!(a.ref_kind, ArgRefKind::Owned)
        && !is_az_string_owned_arg(a, ir)
        && has_wrapper_class(a.type_name.trim(), ir)
}

/// Emit the pre-call wrapper-class conversion lines for one user arg.
/// Mirrors Java's emission: Structure.newInstance + .read() splice over
/// the wrapper's underlying Pointer to produce a `.ByValue` overlay
/// the C ABI accepts.
fn emit_kt_wrapper_class_conv(
    pre_call_lines: &mut Vec<String>,
    raw_name: &str,
    type_name: &str,
) -> String {
    // Strip backticks for the local var prefix (see emit_kt_az_string_conv).
    let stem = raw_name.trim_matches('`');
    let ffi = ffi_type_name(type_name);
    let raw_local = format!("__{}_raw", stem);
    pre_call_lines.push(format!(
        "val {raw_local} = Structure.newInstance({ffi}.ByValue::class.java, {arg}.rawPointer()) \
         as {ffi}.ByValue",
        raw_local = raw_local,
        ffi = ffi,
        arg = raw_name,
    ));
    pre_call_lines.push(format!("{}.read()", raw_local));
    raw_local
}

/// Emit the pre-call AzString conversion lines for one user arg.
/// Mirrors Java's emission: UTF-8 byte buffer + `AzString_fromUtf8`.
fn emit_kt_az_string_conv(pre_call_lines: &mut Vec<String>, raw_name: &str) -> String {
    // Strip backticks if `raw_name` is keyword-escaped (e.g. `` `class` ``).
    // Backticks can't appear inside a compound identifier; they only wrap
    // a whole identifier. Build the local names from the unescaped form.
    let stem = raw_name.trim_matches('`');
    let az_name = format!("__{}_az", stem);
    let bytes_name = format!("__{}_bytes", stem);
    let mem_name = format!("__{}_mem", stem);
    pre_call_lines.push(format!(
        "val {bytes} = {raw}.toByteArray(Charsets.UTF_8)",
        bytes = bytes_name,
        raw = raw_name,
    ));
    // JNA's Memory constructor throws IllegalArgumentException for
    // size 0, so `Button.create("")` etc. must not allocate Memory(0).
    // Allocate at least 1 byte; the native AzString_fromUtf8 (css
    // corety.rs from_utf8) returns AzString::default() whenever
    // len == 0, so the 1-byte buffer is never read and "" round-trips
    // correctly. (Mirrors the lang_java emission.)
    pre_call_lines.push(format!(
        "val {mem} = Memory(maxOf(1, {bytes}.size).toLong())",
        mem = mem_name,
        bytes = bytes_name,
    ));
    pre_call_lines.push(format!(
        "{mem}.write(0, {bytes}, 0, {bytes}.size)",
        mem = mem_name,
        bytes = bytes_name,
    ));
    pre_call_lines.push(format!(
        "val {az} = AzulNativeStr.AzString_fromUtf8({mem}, {bytes}.size.toLong())",
        az = az_name,
        mem = mem_name,
        bytes = bytes_name,
    ));
    az_name
}

/// Phase I.2 (Kotlin): override equals + hashCode routed through the
/// codegen-emitted `Az<X>_partialEq` / `Az<X>_hash` exports when
/// TypeTraits flags them. Mirrors lang_java emission. Pure type-driven.
fn emit_kt_equals_hashcode_if_supported(
    builder: &mut CodeBuilder,
    s: &StructDef,
    class_name: &str,
    _ffi_name: &str,
    ir: &CodegenIR,
) {
    let native = super::super::lang_java::functions::native_class_for_class(&s.name, ir);
    let eq_sym = format!("Az{}_partialEq", s.name);
    let has_eq = s.traits.is_partial_eq && ir.functions.iter().any(|f| f.c_name == eq_sym);
    let hash_sym = format!("Az{}_hash", s.name);
    let has_hash = s.traits.is_hash && ir.functions.iter().any(|f| f.c_name == hash_sym);

    if has_eq {
        builder.line(&format!("/** Equality routed through {}. */", eq_sym));
        builder.line("override fun equals(other: Any?): Boolean {");
        builder.indent();
        builder.line(&format!("if (other !is {}) return false", class_name));
        // `ptr` is a non-nullable `val ptr: Pointer` — a null check here
        // would be an always-false SENSELESS_COMPARISON warning in every
        // user build. Guard on `closed` instead: never touch native
        // memory whose ownership was already transferred/dropped.
        builder.line("if (this.closed || other.closed) return this === other");
        builder.line(&format!(
            "return {}.{}(this.ptr, other.ptr).toInt() != 0",
            native, eq_sym
        ));
        builder.dedent();
        builder.line("}");
        builder.blank();
    }

    if has_hash {
        builder.line(&format!("/** Hash routed through {}. */", hash_sym));
        builder.line("override fun hashCode(): Int {");
        builder.indent();
        // Non-nullable `ptr` — guard on `closed`, not a dead null check.
        builder.line("if (closed) return 0");
        builder.line(&format!("val h = {}.{}(ptr)", native, hash_sym));
        builder.line("return (h xor (h ushr 32)).toInt()");
        builder.dedent();
        builder.line("}");
        builder.blank();
    } else if has_eq {
        // equals compares VALUES (the C `_partialEq`) and the type has no C
        // `_hash`: equal values must hash equal, so the only hash that keeps
        // the contract is a constant (a pointer's hash differs between two
        // equal values).
        builder.line("/** Constant: equal values must hash equal, and the type has no value hash. */");
        builder.line("override fun hashCode(): Int = 0");
        builder.blank();
    }
}

/// The wrapper's ordering surface, routed through the C `Az<X>_cmp` /
/// `Az<X>_partialCmp` exports.
///
/// ORDERING ENCODING (the other side of it is written by `lang_rust`):
/// `0 = Less`, `1 = Equal`, `2 = Greater`, and for `_partialCmp` only
/// `255 = unordered`. Kotlin's `compareTo` wants negative / zero /
/// positive, so "the byte minus one" is an exact re-encoding rather than
/// an invented one.
///
/// Only a TOTAL order (`_cmp`) becomes `Comparable`: a partial one can
/// answer "these two are unordered", which `compareTo` has no way to say,
/// so it gets a nullable method of its own instead of a lie.
fn emit_kt_ordering_if_supported(
    builder: &mut CodeBuilder,
    s: &StructDef,
    class_name: &str,
    ir: &CodegenIR,
) {
    let native = super::super::lang_java::functions::native_class_for_class(&s.name, ir);
    if let Some(sym) = ordering_symbol(s, ir, FunctionKind::Cmp) {
        builder.line(&format!(
            "/** Total order routed through {} (ABI: 0 = Less, 1 = Equal, 2 = Greater). */",
            sym
        ));
        builder.line(&format!(
            "override fun compareTo(other: {}): Int {{",
            class_name
        ));
        builder.indent();
        builder.line("check(!closed && !other.closed) { \"closed\" }");
        builder.line(&format!(
            "return {}.{}(this.ptr, other.ptr).toInt() - 1",
            native, sym
        ));
        builder.dedent();
        builder.line("}");
        builder.blank();
    }
    if let Some(sym) = ordering_symbol(s, ir, FunctionKind::PartialCmp) {
        builder.line(&format!(
            "/** Partial order routed through {}; null when the two are unordered. */",
            sym
        ));
        builder.line(&format!(
            "fun partialCompareTo(other: {}): Int? {{",
            class_name
        ));
        builder.indent();
        builder.line("if (closed || other.closed) return null");
        builder.line(&format!(
            "val __o = {}.{}(this.ptr, other.ptr).toInt() and 0xFF",
            native, sym
        ));
        builder.line("return if (__o > 2) null else __o - 1");
        builder.dedent();
        builder.line("}");
        builder.blank();
    }
}

/// The C symbol of this struct's `_cmp` / `_partialCmp`, when api.json
/// declares the trait AND the export exists. Same two-sided check as
/// `emit_kt_equals_hashcode_if_supported`.
fn ordering_symbol(s: &StructDef, ir: &CodegenIR, kind: FunctionKind) -> Option<String> {
    let declared = match kind {
        FunctionKind::Cmp => s.traits.is_ord,
        FunctionKind::PartialCmp => s.traits.is_partial_ord,
        _ => false,
    };
    if !declared {
        return None;
    }
    ir.functions_for_class(&s.name)
        .find(|f| f.kind == kind)
        .map(|f| f.c_name.clone())
}

/// Phase I.3 (Kotlin): override toString() through Az<X>_toDbgString.
///
/// The engine's STRING type is the one exception: its `toString()` already
/// decodes its own UTF-8 bytes (that is what a caller means by it), so its
/// `Debug` entry point — a DIFFERENT value, the quoted and escaped `{:#?}`
/// form — gets a method of its own rather than being dropped on the floor.
fn emit_kt_to_string_if_supported(builder: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR) {
    let dbg_sym = format!("Az{}_toDbgString", s.name);
    let has_dbg = s.traits.is_debug && ir.functions.iter().any(|f| f.c_name == dbg_sym);
    if !has_dbg {
        return;
    }
    let is_string = matches!(s.category, TypeCategory::String);
    let native = super::super::lang_java::functions::native_class_for_class(&s.name, ir);
    if is_string {
        builder.line(&format!(
            "/** The engine's Debug form (quoted, escaped) routed through {}; `toString()` \
             decodes the bytes themselves. */",
            dbg_sym
        ));
        builder.line("fun debugString(): kotlin.String {");
    } else {
        builder.line(&format!("/** String repr routed through {}. */", dbg_sym));
        builder.line("override fun toString(): kotlin.String {");
    }
    builder.indent();
    // Non-nullable `ptr` — the null half of the old guard was an
    // always-false warning; `closed` is the real lifecycle gate.
    if is_string {
        builder.line("if (closed) return \"\"");
    } else {
        builder.line("if (closed) return super.toString()");
    }
    builder.line(&format!("val __s = {}.{}(ptr)", native, dbg_sym));
    builder.line("__s.write()");
    builder.line("val __sp = __s.pointer");
    builder.line("val __vecPtr: Pointer? = __sp.getPointer(0)");
    builder.line("val __vecLen: Long = __sp.getLong(8)");
    // Free before the early return too: an empty debug string still arrives
    // as an owned AzString, and returning without deleting it leaked one per
    // call.
    builder.line("if (__vecPtr == null || __vecLen <= 0) {");
    builder.line("    AzulNativeStr.AzString_delete(__sp)");
    builder.line("    return \"\"");
    builder.line("}");
    builder.line("val __bytes = __vecPtr.getByteArray(0, __vecLen.toInt())");
    // ByteArray.toString(Charset) avoids the wrapper-class `String`
    // constructor collision (see earlier fix in s.name == \"String\" block).
    builder.line("val __out = __bytes.toString(Charsets.UTF_8)");
    builder.line("AzulNativeStr.AzString_delete(__sp)");
    builder.line("return __out");
    builder.dedent();
    builder.line("}");
    builder.blank();
}

/// Primitive-element Vec sibling of `emit_kt_vec_iterator`: bulk-copy
/// into a Kotlin native typed array (`ByteArray`/`IntArray`/...) via
/// JNA's `getXxxArray`.
fn emit_kt_vec_primitive_array(builder: &mut CodeBuilder, s: &StructDef, elem_rust: &str) {
    let (kt_arr, getter, method_name) = match elem_rust.trim() {
        "u8" | "i8" | "bool" => ("ByteArray", "getByteArray", "toByteArray"),
        "u16" | "i16" => ("ShortArray", "getShortArray", "toShortArray"),
        "u32" | "i32" => ("IntArray", "getIntArray", "toIntArray"),
        "u64" | "i64" | "usize" | "isize" => ("LongArray", "getLongArray", "toLongArray"),
        "f32" => ("FloatArray", "getFloatArray", "toFloatArray"),
        "f64" => ("DoubleArray", "getDoubleArray", "toDoubleArray"),
        _ => return,
    };
    let vec_ffi = ffi_type_name(&s.name);
    builder.line(&format!(
        "/// Bulk-copy the Vec's `{}` elements into a {} (one memcpy, JVM-owned).",
        elem_rust, kt_arr
    ));
    builder.line(&format!("fun {}(): {} {{", method_name, kt_arr));
    builder.indent();
    builder.line(&format!(
        "val __raw = Structure.newInstance({}.ByValue::class.java, ptr) as {}.ByValue",
        vec_ffi, vec_ffi
    ));
    builder.line("__raw.read()");
    // Capture into a local so Kotlin's flow-typing can prove the
    // pointer is non-null after the empty-len short-circuit. JNA
    // exposes Structure fields as mutable, so Kotlin won't smart-
    // cast across the null-check directly.
    builder.line("val __p = __raw.ptr");
    builder.line(&format!(
        "if (__p == null || __raw.len <= 0) return {}(0)",
        kt_arr
    ));
    builder.line(&format!("return __p.{}(0, __raw.len.toInt())", getter));
    builder.dedent();
    builder.line("}");
    builder.blank();
}

/// Iterate the underlying Vec yielding wrapper elements. Each element is
/// deep-cloned via the type's `_clone` C export so the yielded wrapper
/// owns its own heap allocations and survives the Vec being closed. If
/// no `_clone` export exists, fall back to a buffer-borrowed, non-owning
/// wrapper (`owned = false` for types with a `_delete`: no GC-time
/// `AzX_delete` on Vec-internal memory, and the element stays usable).
fn emit_kt_vec_iterator(builder: &mut CodeBuilder, s: &StructDef, elem_type: &str, ir: &CodegenIR) {
    let vec_ffi = ffi_type_name(&s.name);
    let elem_ffi = ffi_type_name(elem_type);
    let elem_wrapper = kotlin_class_name(elem_type, ir);
    let clone_call = format_clone_call_kt(elem_type, ir);
    let elem_has_delete = has_delete_function(elem_type, ir);

    builder.line(&format!(
        "/// Iterate the underlying Vec yielding {} elements.",
        elem_wrapper
    ));
    if clone_call.is_some() {
        builder.line("/// Each element is deep-cloned via _clone; safe past Vec close.");
    } else {
        builder.line(
            "/// Buffer-borrowed iteration (no _clone available); don't keep yielded wrappers \
             past the Vec's lifetime.",
        );
    }
    builder.line(&format!(
        "override fun iterator(): Iterator<{}> {{",
        elem_wrapper
    ));
    builder.indent();
    builder.line(&format!(
        "val __raw = Structure.newInstance({}.ByValue::class.java, ptr) as {}.ByValue",
        vec_ffi, vec_ffi
    ));
    builder.line("__raw.read()");
    builder.line("val __buf = __raw.ptr");
    builder.line("val __n = __raw.len");
    builder.line(&format!(
        "val __sz = Structure.newInstance({}::class.java).size()",
        elem_ffi
    ));
    builder.line(&format!("return object : Iterator<{}> {{", elem_wrapper));
    builder.indent();
    builder.line("private var __i: Long = 0");
    builder.line("override fun hasNext(): Boolean = __i < __n");
    builder.line(&format!("override fun next(): {} {{", elem_wrapper));
    builder.indent();
    builder.line("if (__i >= __n) throw NoSuchElementException()");
    builder.line("val __ep = __buf!!.share(__i * __sz)");
    builder.line("__i++");
    if let Some(ref clone) = clone_call {
        builder.line(&format!(
            "val __cloned = {}(__ep) as {}.ByValue",
            clone, elem_ffi
        ));
        builder.line("__cloned.write()");
        builder.line(&format!("return {}(__cloned.pointer)", elem_wrapper));
    } else {
        builder.line(&format!(
            "val __ev = Structure.newInstance({}.ByValue::class.java, __ep) as {}.ByValue",
            elem_ffi, elem_ffi
        ));
        builder.line("__ev.read()");
        if elem_has_delete {
            builder.line(&format!(
                "return {}(__ev.pointer, owned = false)",
                elem_wrapper
            ));
        } else {
            builder.line(&format!("return {}(__ev.pointer)", elem_wrapper));
        }
    }
    builder.dedent();
    builder.line("}");
    builder.dedent();
    builder.line("}");
    builder.dedent();
    builder.line("}");
    builder.blank();
}

fn emit_static_factory(
    builder: &mut CodeBuilder,
    class_name: &str,
    ffi_name: &str,
    func: &FunctionDef,
    ir: &CodegenIR,
) {
    let method_name = idiomatic_method_name(&func.method_name);

    let return_kt = func
        .return_type
        .as_ref()
        .map(|r| map_kt_return(r, ir))
        .unwrap_or_else(|| "Unit".to_string());

    let returns_self = func
        .return_type
        .as_deref()
        .map(|r| r.trim() == func.class_name)
        .unwrap_or(false);

    let idiom = classify_return(func, ir);

    let arg_sig: Vec<String> = func
        .args
        .iter()
        .map(|a| {
            format!(
                "{}: {}",
                sanitize_kt_identifier(&a.name),
                kt_user_arg_type(a, ir)
            )
        })
        .collect();

    // No wrapper-boundary callback substitution. The wrapper signature
    // carries the C ABI type unchanged; users call AzulHostInvoker.register*
    // themselves to construct the wrapper struct. Matches C# / Java / Lua.
    // Auto-string-conversion + auto-wrapper-class conversion (rules
    // above): Owned `String` args take a kotlin.String; Owned wrapper-
    // class args take the wrapper instance + emit Structure.newInstance
    // splice in pre-call lines.
    let mut pre_call_lines: Vec<String> = Vec::new();
    let mut consume_after_call: Vec<String> = Vec::new();
    let call_args: Vec<String> = func
        .args
        .iter()
        .map(|a| {
            let raw_name = sanitize_kt_identifier(&a.name);
            if is_az_string_owned_arg(a, ir) {
                emit_kt_az_string_conv(&mut pre_call_lines, &raw_name)
            } else if is_wrapper_class_owned_arg(a, ir) {
                let local =
                    emit_kt_wrapper_class_conv(&mut pre_call_lines, &raw_name, a.type_name.trim());
                // The C ABI takes ownership of the by-value struct; an
                // owning wrapper (`_delete`) is marked consumed so its
                // Cleaner never fires a double-free AzX_delete. POD
                // wrappers are copied and stay usable.
                if has_delete_function(a.type_name.trim(), ir) {
                    consume_after_call.push(raw_name);
                }
                local
            } else {
                raw_name
            }
        })
        .collect();

    if !func.doc.is_empty() {
        // (KDoc switched to triple-slash to bypass parser issues with `*/` in inline code samples)
        for d in &func.doc {
            builder.line(&format!("/// {}", kdoc_escape(d)));
        }
    }

    // Auto-wrap non-self wrapper-class returns: same IR-driven
    // predicate as Java's `returns_wrapper_other`.
    let returns_wrapper_other: Option<String> = if returns_self {
        None
    } else if matches!(idiom, ReturnIdiom::Plain) {
        func.return_type
            .as_deref()
            .map(|r| r.trim())
            .filter(|r| has_wrapper_class(r, ir))
            .map(|r| kotlin_class_name(r, ir))
    } else {
        None
    };

    let displayed_return = if returns_self {
        class_name.to_string()
    } else if let Some(ref wrapper) = returns_wrapper_other {
        wrapper.clone()
    } else {
        match &idiom {
            ReturnIdiom::Plain => return_kt.clone(),
            ReturnIdiom::Option {
                payload_ty,
                ref_kind,
            } => {
                let (raw, _) = super::ref_kind_kt_field(payload_ty, ref_kind, ir);
                let display = payload_display_kt(&raw, ir);
                format!("{}?", display)
            }
            ReturnIdiom::Result {
                payload_ty,
                ref_kind,
            } => {
                let (raw, _) = super::ref_kind_kt_field(payload_ty, ref_kind, ir);
                payload_display_kt(&raw, ir)
            }
        }
    };

    builder.line(&format!(
        "@JvmStatic fun {}({}): {} {{",
        method_name,
        arg_sig.join(", "),
        displayed_return
    ));
    builder.indent();

    for stmt in &pre_call_lines {
        builder.line(stmt);
    }

    // Use `managed_c_symbol(func)` to match the AzulNative interface
    // (declared by C ABI symbol with camelCase method portion, and the
    // `<c_name>Struct` triple-variant for callback-wrapper args) rather
    // than `func.method_name` (raw snake-case from api.json) which
    // produces e.g. `AzFoo_with_resolver` instead of `AzFoo_withResolver`.
    let call = format!(
        "{}.{}({})",
        super::super::lang_java::functions::native_class_for_func(func, ir),
        super::super::managed_host_invoker::managed_c_symbol(func),
        call_args.join(", ")
    );

    let emit_consume = |b: &mut CodeBuilder, names: &[String]| {
        for name in names {
            b.line(&format!("{}.__consume()", name));
        }
    };

    if return_kt == "Unit" {
        builder.line(&call.to_string());
        emit_consume(builder, &consume_after_call);
    } else if returns_self {
        // ByValue → adopt its underlying Pointer.
        builder.line(&format!("val raw = {}", call));
        emit_consume(builder, &consume_after_call);
        builder.line(&format!("return {}(raw.pointer)", class_name));
    } else if let Some(ref wrapper) = returns_wrapper_other {
        builder.line(&format!("val raw = {}", call));
        emit_consume(builder, &consume_after_call);
        builder.line(&format!("return {}(raw.pointer)", wrapper));
    } else {
        match &idiom {
            ReturnIdiom::Plain => {
                if consume_after_call.is_empty() {
                    builder.line(&format!("return {}", call));
                } else {
                    builder.line(&format!("val __ret = {}", call));
                    emit_consume(builder, &consume_after_call);
                    builder.line("return __ret");
                }
            }
            ReturnIdiom::Option {
                payload_ty,
                ref_kind,
            } => {
                let (raw, _) = super::ref_kind_kt_field(payload_ty, ref_kind, ir);
                let option_ty = func
                    .return_type
                    .as_deref()
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();
                builder.line(&format!("val __ret = {}", call));
                emit_consume(builder, &consume_after_call);
                emit_kt_option_body(builder, &raw, &option_ty, ir);
            }
            ReturnIdiom::Result {
                payload_ty,
                ref_kind,
            } => {
                let (raw, _) = super::ref_kind_kt_field(payload_ty, ref_kind, ir);
                let result_ty = func
                    .return_type
                    .as_deref()
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();
                builder.line(&format!("val __ret = {}", call));
                emit_consume(builder, &consume_after_call);
                emit_kt_result_body(builder, &raw, &result_ty, ir);
            }
        }
    }

    builder.dedent();
    builder.line("}");
    builder.blank();
}

fn emit_instance_method(
    builder: &mut CodeBuilder,
    class_name: &str,
    ffi_name: &str,
    func: &FunctionDef,
    ir: &CodegenIR,
    app_info: Option<&AppFactoryInfo>,
) {
    let method_name = idiomatic_method_name(&func.method_name);
    // Is this one of the application class's window-opening methods
    // (`run`/`addWindow`)? Then the options arg at `window_arg_idx` gets
    // the factory-registered layout callback spliced in before the call.
    let window_method: Option<(usize, &LayoutCallbackFactoryInfo)> = app_info
        .filter(|i| i.class_name == func.class_name)
        .and_then(|i| {
            i.window_methods
                .iter()
                .find(|(c, _, _)| *c == func.c_name)
                .map(|(_, idx, w)| (*idx, w))
        });

    let return_kt = func
        .return_type
        .as_ref()
        .map(|r| map_kt_return(r, ir))
        .unwrap_or_else(|| "Unit".to_string());

    let returns_self = func
        .return_type
        .as_deref()
        .map(|r| r.trim() == func.class_name)
        .unwrap_or(false);

    // Drop the implicit self argument. For instance methods (caller path
    // is `emit_instance_method` so takes_self is always true here) the
    // first arg in func.args IS the self regardless of how api.json names
    // it (`instance`, lowercased class name, `icon_provider_handle`,
    // etc.). Skip args[0] unconditionally — matches the Java/C# fix.
    let user_args: Vec<_> = func.args.iter().skip(1).collect();

    // Some C ABIs take self by VALUE (`AzRibbon_renderDom(AzRibbon)`)
    // rather than by pointer. Detect via args[0].ref_kind = Owned and
    // build a `.ByValue` overlay via JNA's Structure.newInstance(...).
    let self_by_value = func
        .args
        .first()
        .map(|a| matches!(a.ref_kind, ArgRefKind::Owned))
        .unwrap_or(false);

    let arg_sig: Vec<String> = user_args
        .iter()
        .map(|a| {
            format!(
                "{}: {}",
                sanitize_kt_identifier(&a.name),
                kt_user_arg_type(a, ir)
            )
        })
        .collect();

    let mut pre_call_lines: Vec<String> = Vec::new();
    let mut consume_after_call: Vec<String> = Vec::new();
    let self_arg = if self_by_value {
        let self_ty = format!("Az{}", func.class_name);
        pre_call_lines.push(format!(
            "val __self = Structure.newInstance({}.ByValue::class.java, this.ptr) as {}.ByValue",
            self_ty, self_ty
        ));
        pre_call_lines.push("__self.read()".to_string());
        // DeepCopy / consuming-self method: the C side takes the bytes.
        // Only an owning wrapper needs its Cleaner cancelled.
        if has_delete_function(&func.class_name, ir) {
            consume_after_call.push("this".to_string());
        }
        "__self".to_string()
    } else {
        "this.ptr".to_string()
    };

    let mut call_args: Vec<String> = vec![self_arg];
    // Auto-string-conversion + auto-wrapper-class conversion: Owned
    // `String` args take a kotlin.String + AzString_fromUtf8 splice;
    // Owned wrapper-class args take the wrapper instance + Structure.
    // newInstance splice. Pure type-driven (see top-of-file predicates).
    for (j, a) in user_args.iter().enumerate() {
        let raw_name = sanitize_kt_identifier(&a.name);
        let value = if is_az_string_owned_arg(a, ir) {
            emit_kt_az_string_conv(&mut pre_call_lines, &raw_name)
        } else if is_wrapper_class_owned_arg(a, ir) {
            let raw_local =
                emit_kt_wrapper_class_conv(&mut pre_call_lines, &raw_name, a.type_name.trim());
            if has_delete_function(a.type_name.trim(), ir) {
                consume_after_call.push(raw_name);
            }
            raw_local
        } else {
            raw_name
        };
        // Window-opening method of the application class: splice the
        // `create(data, fn)` callback into the options struct overlay.
        if let Some((_, w)) = window_method.filter(|(idx, _)| *idx == j + 1) {
            pre_call_lines.push(format!("__spliceInto{}({})", w.class_name, value));
        }
        call_args.push(value);
    }

    if !func.doc.is_empty() {
        // (KDoc switched to triple-slash to bypass parser issues with `*/` in inline code samples)
        for d in &func.doc {
            builder.line(&format!("/// {}", kdoc_escape(d)));
        }
    }

    let idiom = classify_return(func, ir);

    let returns_wrapper_other: Option<String> = if returns_self {
        None
    } else if matches!(idiom, ReturnIdiom::Plain) {
        func.return_type
            .as_deref()
            .map(|r| r.trim())
            .filter(|r| has_wrapper_class(r, ir))
            .map(|r| kotlin_class_name(r, ir))
    } else {
        None
    };

    let displayed_return = if returns_self {
        class_name.to_string()
    } else if let Some(ref wrapper) = returns_wrapper_other {
        wrapper.clone()
    } else {
        match &idiom {
            ReturnIdiom::Plain => return_kt.clone(),
            ReturnIdiom::Option {
                payload_ty,
                ref_kind,
            } => {
                let (raw, _) = super::ref_kind_kt_field(payload_ty, ref_kind, ir);
                let display = payload_display_kt(&raw, ir);
                format!("{}?", display)
            }
            ReturnIdiom::Result {
                payload_ty,
                ref_kind,
            } => {
                let (raw, _) = super::ref_kind_kt_field(payload_ty, ref_kind, ir);
                payload_display_kt(&raw, ir)
            }
        }
    };

    builder.line(&format!(
        "fun {}({}): {} {{",
        method_name,
        arg_sig.join(", "),
        displayed_return
    ));
    builder.indent();
    builder.line("check(!closed) { \"closed\" }");

    for stmt in &pre_call_lines {
        builder.line(stmt);
    }

    // Use `managed_c_symbol(func)` to match the AzulNative interface
    // (declared by C ABI symbol with camelCase method portion, and the
    // `<c_name>Struct` triple-variant for callback-wrapper args) rather
    // than `func.method_name` (raw snake-case from api.json) which
    // produces e.g. `AzFoo_with_resolver` instead of `AzFoo_withResolver`.
    let call = format!(
        "{}.{}({})",
        super::super::lang_java::functions::native_class_for_func(func, ir),
        super::super::managed_host_invoker::managed_c_symbol(func),
        call_args.join(", ")
    );

    let emit_consume = |b: &mut CodeBuilder, names: &[String]| {
        for name in names {
            b.line(&format!("{}.__consume()", name));
        }
    };

    if return_kt == "Unit" {
        builder.line(&call.to_string());
        emit_consume(builder, &consume_after_call);
    } else if returns_self {
        builder.line(&format!("val raw = {}", call));
        emit_consume(builder, &consume_after_call);
        builder.line(&format!("return {}(raw.pointer)", class_name));
    } else if let Some(ref wrapper) = returns_wrapper_other {
        builder.line(&format!("val raw = {}", call));
        emit_consume(builder, &consume_after_call);
        builder.line(&format!("return {}(raw.pointer)", wrapper));
    } else {
        match &idiom {
            ReturnIdiom::Plain => {
                if consume_after_call.is_empty() {
                    builder.line(&format!("return {}", call));
                } else {
                    builder.line(&format!("val __ret = {}", call));
                    emit_consume(builder, &consume_after_call);
                    builder.line("return __ret");
                }
            }
            ReturnIdiom::Option {
                payload_ty,
                ref_kind,
            } => {
                let (raw, _) = super::ref_kind_kt_field(payload_ty, ref_kind, ir);
                let option_ty = func
                    .return_type
                    .as_deref()
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();
                builder.line(&format!("val __ret = {}", call));
                emit_consume(builder, &consume_after_call);
                emit_kt_option_body(builder, &raw, &option_ty, ir);
            }
            ReturnIdiom::Result {
                payload_ty,
                ref_kind,
            } => {
                let (raw, _) = super::ref_kind_kt_field(payload_ty, ref_kind, ir);
                let result_ty = func
                    .return_type
                    .as_deref()
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();
                builder.line(&format!("val __ret = {}", call));
                emit_consume(builder, &consume_after_call);
                emit_kt_result_body(builder, &raw, &result_ty, ir);
            }
        }
    }

    builder.dedent();
    builder.line("}");
    builder.blank();

    // Smart callback setters (`onClick(data, rawSam)` + `withOnClick(data: T,
    // typedSam)`) follow the plain method as siblings, never inside it.
    emit_kt_smart_setters(builder, func, &displayed_return, ir);
}

fn emit_union_helper(builder: &mut CodeBuilder, e: &EnumDef, ir: &CodegenIR) {
    let class_name = kotlin_class_name(&e.name, ir);
    let ffi_name = ffi_type_name(&e.name);

    if !e.doc.is_empty() {
        // (KDoc switched to triple-slash to bypass parser issues with `*/` in inline code samples)
        for d in &e.doc {
            builder.line(&format!("/// {}", kdoc_escape(d)));
        }
    }

    builder.line(&format!("object {}Helpers {{", class_name));
    builder.indent();

    for v in &e.variants {
        match &v.kind {
            EnumVariantKind::Unit => {
                builder.line(&format!(
                    "/** Construct the {}.{} variant. */",
                    e.name, v.name
                ));
                // libazul exports a constructor for the unit variants too
                // (`AzAccessibilityAction_blur()`). Calling it keeps ONE
                // place that knows the `repr(C, u8)` layout: the engine.
                // Building the union by hand here worked, but it meant the
                // export was declared and never called, and it would go
                // quietly wrong the day a variant grows a payload or the
                // tag widens.
                if let Some(ctor) = ir.variant_constructor(&e.name, &v.name) {
                    builder.line(&format!(
                        "@JvmStatic fun {}(): {} = {}.{}()",
                        idiomatic_method_name(&v.name),
                        super::map_kt_return(&e.name, ir),
                        super::super::lang_java::functions::native_class_for_func(ctor, ir),
                        super::super::managed_host_invoker::managed_c_symbol(ctor),
                    ));
                    builder.blank();
                    continue;
                }
                // No export for this variant: set the tag ourselves.
                let variant_ident = sanitize_kt_identifier(&v.name);
                builder.line(&format!(
                    "@JvmStatic fun {}(): {} {{",
                    idiomatic_method_name(&v.name),
                    ffi_name
                ));
                builder.indent();
                builder.line(&format!("val u = {}()", ffi_name));
                // `.value` is Int; AzX_Tag is repr(C, u8) so the tag
                // field is `Byte`. Cast explicitly — Kotlin doesn't
                // implicitly narrow Int → Byte.
                builder.line(&format!(
                    "u.{}.tag = {}_Tag.{}.value.toByte()",
                    variant_ident, ffi_name, variant_ident
                ));
                builder.line(&format!("u.setType(\"{}\")", v.name));
                builder.line("return u");
                builder.dedent();
                builder.line("}");
                builder.blank();
            }
            EnumVariantKind::Tuple(_) | EnumVariantKind::Struct(_) => {
                // A payload variant: the C constructor libazul exports for it
                // (`Az{Enum}_{variant}(payload)`) builds the tagged union.
                let Some(ctor) = ir.variant_constructor(&e.name, &v.name) else {
                    continue;
                };
                let params: Vec<String> = ctor
                    .args
                    .iter()
                    .map(|a| {
                        format!(
                            "{}: {}",
                            sanitize_kt_identifier(&a.name),
                            super::map_kt_owned(&a.type_name, ir)
                        )
                    })
                    .collect();
                let names: Vec<String> =
                    ctor.args.iter().map(|a| sanitize_kt_identifier(&a.name)).collect();
                builder.line(&format!(
                    "/** Construct the {}.{} variant. */",
                    e.name, v.name
                ));
                builder.line(&format!(
                    "@JvmStatic fun {}({}): {} = {}.{}({})",
                    idiomatic_method_name(&v.name),
                    params.join(", "),
                    super::map_kt_return(&e.name, ir),
                    super::super::lang_java::functions::native_class_for_func(ctor, ir),
                    super::super::managed_host_invoker::managed_c_symbol(ctor),
                    names.join(", ")
                ));
                builder.blank();
            }
        }
    }

    builder.dedent();
    builder.line("}");
    builder.blank();
}

pub(super) fn idiomatic_method_name(method_name: &str) -> String {
    if method_name == "new" {
        return "create".to_string();
    }
    let camel = if method_name.contains('_') {
        let mut out = String::new();
        let mut upper = false;
        for c in method_name.chars() {
            if c == '_' {
                upper = true;
            } else if upper {
                out.extend(c.to_uppercase());
                upper = false;
            } else {
                out.push(c);
            }
        }
        out
    } else {
        let mut chars = method_name.chars();
        match chars.next() {
            Some(c) => c.to_lowercase().collect::<String>() + chars.as_str(),
            None => String::new(),
        }
    };
    // Kotlin hard keywords (`object`, `class`, `interface`, etc.) cannot
    // be method names without backticks; emit them backticked. (Backticks
    // are valid inside method-name position in Kotlin source.)
    if super::is_kotlin_hard_keyword(&camel) {
        format!("`{}`", camel)
    } else if camel == "close" {
        // Every wrapper implements AutoCloseable with its own `close()`
        // for resource cleanup. A user-API method also named `close`
        // would collide; rename it. (SvgPath has both — the path's "close
        // path" segment plus the AutoCloseable.close() lifecycle method.)
        "closeInner".to_string()
    } else if matches!(camel.as_str(), "toString" | "hashCode" | "equals") {
        // Methods on Any/Object require an `override` modifier and
        // a compatible return type. The Azul wrappers' `toString` returns
        // AzString.ByValue, not java.lang.String, so it can't override
        // Any.toString. Suffix to avoid the collision.
        format!("{}_", camel)
    } else {
        camel
    }
}

/// Escape doc-comment text for KDoc emission. Several characters in
/// the raw Rust docs would otherwise confuse Kotlin's parser:
///
/// - `*/` inside paths like `/users/*/name` is read as the doc-comment terminator, prematurely
///   closing the KDoc and surfacing as "Missing '}" / "Unclosed comment" errors on later lines.
/// - `{` / `}` are KDoc inline-tag delimiters. Unbalanced braces from inline code samples
///   (`r#"{"users":...}"#`) trip the doc parser.
pub(crate) fn kdoc_escape(s: &str) -> String {
    s.replace("*/", "*&#47;")
        .replace('{', "&#123;")
        .replace('}', "&#125;")
}
