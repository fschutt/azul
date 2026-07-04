//! Kotlin-idiomatic wrapper-class emission.
//!
//! For every IR struct that has a matching `<TypeName>_delete` C
//! function we emit:
//!
//! ```kotlin
//! class App private constructor(private val ptr: Pointer) : AutoCloseable {
//!     companion object {
//!         fun create(...): App = App(AzulNative.INSTANCE.AzApp_create(...).pointer!!)
//!     }
//!     override fun close() = AzulNative.INSTANCE.AzApp_delete(ptr)
//!     fun run(...) = AzulNative.INSTANCE.AzApp_run(ptr, ...)
//! }
//! ```
//!
//! Kotlin's stdlib already provides `AutoCloseable.use { }` so call
//! sites get `App.create(...).use { app -> app.run(...) }` for free.
//!
//! Tagged-union enums get a tiny helper `object` with static factory
//! methods per unit variant; payload-bearing variants are left to the
//! user (`Az<Foo>` Union + `Az<Foo>Variant_<Variant>` payload struct).

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{
    ArgRefKind, CodegenIR, EnumDef, EnumVariantKind, FieldRefKind, FunctionArg, FunctionDef,
    FunctionKind, MonomorphizedKind, StructDef, TypeCategory,
};
use super::{ffi_type_name, kotlin_class_name, map_kt_owned, map_kt_return, sanitize_kt_identifier};

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
                                ref_kind: sv.payload_ref_kind.clone(),
                            };
                        }
                    }
                    let ok = variants.iter().find(|v| v.name == "Ok");
                    let err = variants.iter().find(|v| v.name == "Err");
                    if let (Some(ov), Some(_)) = (ok, err) {
                        if let Some(ref pt) = ov.payload_type {
                            return ReturnIdiom::Result {
                                payload_ty: pt.clone(),
                                ref_kind: ov.payload_ref_kind.clone(),
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
                            ref_kind: types[0].1.clone(),
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
                            ref_kind: types[0].1.clone(),
                        };
                    }
                }
            }
        }
    }
    ReturnIdiom::Plain
}

/// True iff `type_name` has a corresponding `class <X> : AutoCloseable`
/// wrapper emitted by `emit_wrapper`. Enums (`CssDeclaration`,
/// `AccessibilityAction`, ...) are emitted as `<X>Helpers` static-
/// factory objects with no constructor-taking-Pointer.
fn has_kt_wrapper_class(type_name: &str, ir: &CodegenIR) -> bool {
    let Some(s) = ir.find_struct(type_name) else {
        return false;
    };
    if !s.generic_params.is_empty() {
        return false;
    }
    if matches!(
        s.category,
        TypeCategory::Recursive
            | TypeCategory::VecRef
            | TypeCategory::DestructorOrClone
            | TypeCategory::GenericTemplate
    ) {
        return false;
    }
    ir.functions
        .iter()
        .any(|f| f.class_name == type_name && matches!(f.kind, FunctionKind::Delete))
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
        if has_kt_wrapper_class(unprefixed, ir) {
            return unprefixed.to_string();
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

/// Build an `<NativeClass>.INSTANCE.Az<OptionT>_delete(__ret.getPointer())`
/// call (or None when there's no _delete export).
fn format_option_delete_call_kt(option_type_name: &str, ir: &CodegenIR) -> Option<String> {
    use super::super::ir::FunctionKind;
    let has_delete = ir
        .functions
        .iter()
        .any(|f| f.class_name == option_type_name && matches!(f.kind, FunctionKind::Delete));
    if !has_delete {
        return None;
    }
    let native =
        super::super::lang_java::functions::native_class_for_class(option_type_name, ir);
    let ffi_name = ffi_type_name(option_type_name);
    Some(format!(
        "{}.INSTANCE.{}_delete(__ret.getPointer())",
        native, ffi_name
    ))
}

/// Build an `<NativeClass>.INSTANCE.Az<T>_clone` expression for a
/// wrapper-class payload type, or None if no _clone export exists.
fn format_clone_call_kt(payload_type_name: &str, ir: &CodegenIR) -> Option<String> {
    use super::super::ir::FunctionKind;
    let has_clone = ir.functions.iter().any(|f| {
        f.class_name == payload_type_name && matches!(f.kind, FunctionKind::DeepCopy)
    });
    if !has_clone {
        return None;
    }
    let native =
        super::super::lang_java::functions::native_class_for_class(payload_type_name, ir);
    let ffi_name = ffi_type_name(payload_type_name);
    Some(format!("{}.INSTANCE.{}_clone", native, ffi_name))
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
        if has_kt_wrapper_class(unprefixed, ir) {
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
                builder.line(&format!("return {}(__cloned.pointer)", unprefixed));
            } else {
                builder.line(&format!("return {}(__nv.pointer)", unprefixed));
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
        if has_kt_wrapper_class(unprefixed, ir) {
            let clone_call = format_clone_call_kt(unprefixed, ir);
            builder.line("val __u = __ret.unwrap()");
            if let Some(ref clone) = clone_call {
                builder.line(&format!(
                    "val __cloned = {}(__u.pointer) as {}.ByValue",
                    clone, raw_payload_kt
                ));
                builder.line("__cloned.write()");
                emit_delete(builder);
                builder.line(&format!("return {}(__cloned.pointer)", unprefixed));
            } else {
                builder.line(&format!("return {}(__u.pointer)", unprefixed));
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

    for s in &ir.structs {
        if !should_emit_wrapper(s, ir, config) {
            continue;
        }
        emit_wrapper(builder, s, ir);
    }

    for e in &ir.enums {
        if !should_emit_helper(e, config) {
            continue;
        }
        emit_union_helper(builder, e);
    }

    Ok(())
}

fn should_emit_wrapper(s: &StructDef, ir: &CodegenIR, config: &CodegenConfig) -> bool {
    if !config.should_include_type(&s.name) {
        return false;
    }
    if !s.generic_params.is_empty() {
        return false;
    }
    if matches!(
        s.category,
        TypeCategory::Recursive
            | TypeCategory::VecRef
            | TypeCategory::DestructorOrClone
            | TypeCategory::GenericTemplate
    ) {
        return false;
    }
    has_delete(&s.name, ir)
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

fn has_delete(type_name: &str, ir: &CodegenIR) -> bool {
    ir.functions
        .iter()
        .any(|f| f.class_name == type_name && f.kind == FunctionKind::Delete)
}

/// Phase I.1.3 (Kotlin): Vec-shape detector. Same predicate as Haskell
/// H.3 / Ruby I.1.6 / Java I.1.2.
fn detect_vec_elem_type_kt(s: &StructDef) -> Option<String> {
    if s.fields.len() != 4 {
        return None;
    }
    if s.fields[0].name != "ptr"
        || s.fields[1].name != "len"
        || s.fields[2].name != "cap"
    {
        return None;
    }
    if s.fields[1].type_name.trim() != "usize"
        || s.fields[2].type_name.trim() != "usize"
    {
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

fn emit_wrapper(builder: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR) {
    let class_name = kotlin_class_name(&s.name);
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
    let vec_elem_has_wrapper = |elem: &str| -> bool {
        ir.find_struct(elem).is_some()
            && ir.functions.iter().any(|f| {
                f.class_name == elem && f.kind == FunctionKind::Delete
            })
    };
    let extra_iface = match &vec_elem_type {
        Some(elem) if vec_elem_has_wrapper(elem) => {
            format!(", Iterable<{}>", kotlin_class_name(elem))
        }
        _ => String::new(),
    };

    // `internal constructor`: accessible from sibling classes in the
    // same Kotlin module (this file), which is where smart factories
    // and auto-wrapper-class converted call sites construct wrappers
    // from raw pointers. Users outside the module use the static
    // factories.
    builder.line(&format!(
        "class {} internal constructor(internal val ptr: Pointer) : AutoCloseable{} {{",
        class_name, extra_iface
    ));
    builder.indent();

    builder.line("private var closed: Boolean = false");
    builder.blank();

    // Internal pointer access for sibling wrappers.
    builder.line("/** Internal: raw pointer for use by sibling wrappers. */");
    builder.line("fun rawPointer(): Pointer = ptr");
    builder.blank();

    // AzString gets a `toString()` override that decodes the wrapped
    // UTF-8 bytes into a `kotlin.String`. AzString's C-side layout is
    // `{ vec: AzU8Vec }`, AzU8Vec is `{ ptr, len, cap, destructor }`,
    // so offset 0 is `vec.ptr` (the UTF-8 byte buffer) and offset 8 is
    // `vec.len` (byte length).
    // Phase J.1 (Kotlin): same shared detector as Java drives the smart
    // `<event>(data, fn)` factory for every method matching the
    // `with_on_*(self, RefAny, <CallbackWrapperStruct>)` shape.
    for func in ir.functions_for_class(&s.name) {
        let Some((smart_snake, wrapper_kind)) =
            super::super::managed_host_invoker::smart_callback_setter_info(func)
        else {
            continue;
        };
        let smart_camel = super::super::lang_java::snake_to_lower_camel(&smart_snake);
        let with_camel = idiomatic_method_name(&func.method_name);
        let sam_class = format!("AzulNativeManaged.{}InvokerCallback", wrapper_kind);
        let register_method = if wrapper_kind == "Callback" {
            "registerCallback".to_string()
        } else {
            format!("register{}", wrapper_kind)
        };
        builder.line("/**");
        builder.line(&format!(
            " * Smart builder for {}: SAM + host object. Auto-registers via",
            with_camel
        ));
        builder.line(" * AzulHostInvoker.");
        builder.line(" */");
        builder.line(&format!(
            "fun {}(data: Any, fn: {}): {} {{",
            smart_camel, sam_class, class_name
        ));
        builder.indent();
        builder.line("val __data = AzulHostInvoker.refanyCreate(data)");
        builder.line(&format!(
            "val __cb = AzulHostInvoker.{}(fn)",
            register_method
        ));
        builder.line(&format!(
            "return {}(RefAny(__data.pointer), {}(__cb.pointer))",
            with_camel, wrapper_kind
        ));
        builder.dedent();
        builder.line("}");
        builder.blank();
    }

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

    // WindowCreateOptions.create(layout fn) — smart factory; see the
    // Java mirror for the full rationale. Kotlin emits the body inside
    // the companion object alongside the rest of the static factories,
    // but the codegen path here is straightforward enough to inline.
    // We append the create() to the companion object via a marker
    // method invoked from the companion emission later. For now,
    // emit it as a top-level method on the wrapper itself; Kotlin
    // resolves `WindowCreateOptions.create(...)` to a member method
    // when called from another class.
    //
    // (Companion-object emission would be more idiomatic but the
    // existing codegen-emitted companion lives in `static_funcs` loop
    // below, separate from this hook — so we open the companion
    // ourselves here.)
    // companion object holding the static factories.
    let static_funcs: Vec<&FunctionDef> = ir
        .functions_for_class(&s.name)
        .filter(|f| {
            !f.kind.is_trait_function()
                && matches!(
                    f.kind,
                    FunctionKind::Constructor
                        | FunctionKind::StaticMethod
                        | FunctionKind::Default
                )
        })
        .collect();

    // Smart-factory hook (must be inside the companion object). Pre-
    // populated for WindowCreateOptions; sibling bindings follow the
    // same pattern. Opens the companion even when `static_funcs` is
    // empty, so a wrapper that has ONLY a smart factory still gets a
    // companion object emitted.
    let layout_factory_info =
        super::super::managed_host_invoker::layout_callback_factory_info(s, ir);
    let needs_companion = !static_funcs.is_empty() || layout_factory_info.is_some();
    if needs_companion {
        builder.line("companion object {");
        builder.indent();
        if let Some(info) = layout_factory_info.as_ref() {
            let wrapper_class = kotlin_class_name(&info.class_name);
            let ffi_class = ffi_type_name(&info.class_name);
            let cb_ffi = ffi_type_name(&info.callback_wrapper);
            let register_fn = format!("register{}", info.callback_wrapper);
            let native_class =
                super::super::lang_java::functions::native_class_for_class(&info.class_name, ir);
            let field_path = info.field_path.join(".");
            let sam_raw = format!(
                "AzulNativeManaged.{}InvokerCallback",
                info.callback_wrapper
            );
            let sam_typed = format!("AzulHostInvoker.{}", info.callback_wrapper);

            for (sam_type, doc_note) in [
                (
                    sam_raw.as_str(),
                    "Smart factory: pass a layout-callback lambda; the host-invoker registration and bytes-copy plumbing happen internally.",
                ),
                (
                    sam_typed.as_str(),
                    "Smart factory (typed): pass a typed callback that returns a wrapper directly; the bridge splices the bytes into the embedded callback field.",
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
                builder.line(&format!(
                    "val __cb = AzulHostInvoker.{}(fn)",
                    register_fn
                ));
                builder.line(&format!(
                    "val __wco = {}.INSTANCE.{}()",
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
                builder.line(&format!(
                    "return {}(__wco.getPointer())",
                    wrapper_class
                ));
                builder.dedent();
                builder.line("}");
                builder.blank();
                let _ = (ffi_class.as_str(), cb_ffi.as_str()); // referenced via register/default; keep names alive for IR-driven debugging
            }
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
        emit_instance_method(builder, &class_name, &ffi_name, func, ir);
    }

    // Phase I.2 (Kotlin): equals + hashCode routed through the
    // codegen-emitted C-ABI helpers when TypeTraits says they exist.
    emit_kt_equals_hashcode_if_supported(builder, s, &class_name, &ffi_name, ir);

    // Phase I.3 (Kotlin): toString() routed through Az<X>_toDbgString.
    emit_kt_toString_if_supported(builder, s, ir);

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

    // close()
    builder.line("/** Frees the underlying native resources. Idempotent. */");
    builder.line("override fun close() {");
    builder.indent();
    builder.line("if (closed) return");
    builder.line(&format!(
        "{}.INSTANCE.{}_delete(ptr)",
        super::super::lang_java::functions::native_class_for_class(&s.name, ir),
        ffi_name
    ));
    builder.line("closed = true");
    builder.dedent();
    builder.line("}");
    builder.blank();

    // Mark this wrapper as consumed without calling Az<X>_delete.
    // Used by codegen-emitted call sites where the C ABI takes
    // ownership of the underlying bytes by-value.
    builder.line("/** Internal: mark consumed (called by codegen-emitted bridges that transfer ownership to the C ABI by-value). */");
    builder.line("internal fun __consume() {");
    builder.indent();
    builder.line("closed = true");
    builder.dedent();
    builder.line("}");

    builder.dedent();
    builder.line("}");
    builder.blank();
}

/// Auto-string-conversion rule: any Owned `String` arg at the C ABI
/// accepts a `kotlin.String` at the wrapper level. Returns true if this
/// arg should be re-typed to `kotlin.String` and converted in pre-call
/// lines. Pure type-driven; no method-name allowlist.
fn is_az_string_owned_arg(a: &FunctionArg) -> bool {
    a.type_name.trim() == "String" && matches!(a.ref_kind, ArgRefKind::Owned)
}

/// Auto-wrapper-class rule: any Owned arg whose type matches an
/// emitted wrapper struct accepts the wrapper instance at the param;
/// pre-call splice writes the underlying Pointer into a
/// `.ByValue` Structure overlay so the C ABI sees a real struct
/// value. Pure type-driven; no method-name allowlist.
///
/// Strict: only true when the codegen actually emits a wrapper class
/// for the named type (has a delete function + not excluded by
/// category). Otherwise we'd reference classes that don't exist.
fn is_wrapper_class_owned_arg(a: &FunctionArg, ir: &CodegenIR) -> bool {
    if !matches!(a.ref_kind, ArgRefKind::Owned) {
        return false;
    }
    let tn = a.type_name.trim();
    if tn == "String" {
        return false;
    }
    let Some(s) = ir.find_struct(tn) else {
        return false;
    };
    if !s.generic_params.is_empty() {
        return false;
    }
    if matches!(
        s.category,
        super::super::ir::TypeCategory::Recursive
            | super::super::ir::TypeCategory::VecRef
            | super::super::ir::TypeCategory::DestructorOrClone
            | super::super::ir::TypeCategory::GenericTemplate
    ) {
        return false;
    }
    // Kotlin's wrapper-class emission uses the same has_delete_function
    // gate as Java's. Probe it directly.
    ir.functions.iter().any(|f| {
        f.class_name == tn && matches!(f.kind, FunctionKind::Delete)
    })
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
        "val {raw_local} = Structure.newInstance({ffi}.ByValue::class.java, {arg}.rawPointer()) as {ffi}.ByValue",
        raw_local = raw_local,
        ffi = ffi,
        arg = raw_name,
    ));
    pre_call_lines.push(format!("{}.read()", raw_local));
    raw_local
}

/// Emit the pre-call AzString conversion lines for one user arg.
/// Mirrors Java's emission: UTF-8 byte buffer + `AzString_fromUtf8`.
fn emit_kt_az_string_conv(
    pre_call_lines: &mut Vec<String>,
    raw_name: &str,
) -> String {
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
        "val {az} = AzulNativeStr.INSTANCE.AzString_fromUtf8({mem}, {bytes}.size.toLong())",
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
    let has_eq = s.traits.is_partial_eq
        && ir.functions.iter().any(|f| f.c_name == eq_sym);
    let hash_sym = format!("Az{}_hash", s.name);
    let has_hash = s.traits.is_hash
        && ir.functions.iter().any(|f| f.c_name == hash_sym);

    if has_eq {
        builder.line(&format!(
            "/** Equality routed through {}. */",
            eq_sym
        ));
        builder.line("override fun equals(other: Any?): Boolean {");
        builder.indent();
        builder.line(&format!("if (other !is {}) return false", class_name));
        // `ptr` is a non-nullable `val ptr: Pointer` — a null check here
        // would be an always-false SENSELESS_COMPARISON warning in every
        // user build. Guard on `closed` instead: never touch native
        // memory whose ownership was already transferred/dropped.
        builder.line("if (this.closed || other.closed) return this === other");
        builder.line(&format!(
            "return {}.INSTANCE.{}(this.ptr, other.ptr).toInt() != 0",
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
        builder.line(&format!(
            "val h = {}.INSTANCE.{}(ptr)",
            native, hash_sym
        ));
        builder.line("return (h xor (h ushr 32)).toInt()");
        builder.dedent();
        builder.line("}");
        builder.blank();
    } else if has_eq {
        // equals/hashCode contract: when equals is overridden, hashCode
        // must be too. Fall back to identity.
        // `ptr` is non-nullable — `ptr?.` would emit an
        // UNNECESSARY_SAFE_CALL warning in every user build.
        builder.line("override fun hashCode(): Int = ptr.hashCode()");
        builder.blank();
    }
}

/// Phase I.3 (Kotlin): override toString() through Az<X>_toDbgString.
fn emit_kt_toString_if_supported(
    builder: &mut CodeBuilder,
    s: &StructDef,
    ir: &CodegenIR,
) {
    if matches!(s.category, TypeCategory::String) {
        return; // Vec-direct decode already in place.
    }
    let dbg_sym = format!("Az{}_toDbgString", s.name);
    let has_dbg = s.traits.is_debug
        && ir.functions.iter().any(|f| f.c_name == dbg_sym);
    if !has_dbg {
        return;
    }
    let native = super::super::lang_java::functions::native_class_for_class(&s.name, ir);
    builder.line(&format!("/** String repr routed through {}. */", dbg_sym));
    builder.line("override fun toString(): kotlin.String {");
    builder.indent();
    // Non-nullable `ptr` — the null half of the old guard was an
    // always-false warning; `closed` is the real lifecycle gate.
    builder.line("if (closed) return super.toString()");
    builder.line(&format!(
        "val __s = {}.INSTANCE.{}(ptr)",
        native, dbg_sym
    ));
    builder.line("__s.write()");
    builder.line("val __sp = __s.pointer");
    builder.line("val __vecPtr: Pointer? = __sp.getPointer(0)");
    builder.line("val __vecLen: Long = __sp.getLong(8)");
    builder.line("if (__vecPtr == null || __vecLen <= 0) return \"\"");
    builder.line("val __bytes = __vecPtr.getByteArray(0, __vecLen.toInt())");
    // ByteArray.toString(Charset) avoids the wrapper-class `String`
    // constructor collision (see earlier fix in s.name == \"String\" block).
    builder.line("val __out = __bytes.toString(Charsets.UTF_8)");
    builder.line("AzulNativeStr.INSTANCE.AzString_delete(__sp)");
    builder.line("return __out");
    builder.dedent();
    builder.line("}");
    builder.blank();
}

/// Iterate the underlying Vec yielding wrapper elements. Each
/// element is deep-cloned via the type's `_clone` C export so the
/// yielded wrapper owns its own heap allocations and survives the
/// Vec being closed. If no `_clone` export exists, fall back to a
/// buffer-borrowed wrapper marked consumed (no finalize-time
/// `AzX_delete` on Vec-internal memory).
/// Primitive-element Vec sibling: bulk-copy into a Kotlin native
/// typed array (`ByteArray`/`IntArray`/...) via JNA's `getXxxArray`.
fn emit_kt_vec_primitive_array(
    builder: &mut CodeBuilder,
    s: &StructDef,
    elem_rust: &str,
) {
    let (kt_arr, getter, method_name) = match elem_rust.trim() {
        "u8" | "i8" | "bool" => ("ByteArray", "getByteArray", "toByteArray"),
        "u16" | "i16" => ("ShortArray", "getShortArray", "toShortArray"),
        "u32" | "i32" => ("IntArray", "getIntArray", "toIntArray"),
        "u64" | "i64" | "usize" | "isize" => {
            ("LongArray", "getLongArray", "toLongArray")
        }
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
    builder.line(&format!(
        "return __p.{}(0, __raw.len.toInt())",
        getter
    ));
    builder.dedent();
    builder.line("}");
    builder.blank();
}

fn emit_kt_vec_iterator(
    builder: &mut CodeBuilder,
    s: &StructDef,
    elem_type: &str,
    ir: &CodegenIR,
) {
    let vec_ffi = ffi_type_name(&s.name);
    let elem_ffi = ffi_type_name(elem_type);
    let elem_wrapper = kotlin_class_name(elem_type);
    let clone_call = format_clone_call_kt(elem_type, ir);

    builder.line(&format!(
        "/// Iterate the underlying Vec yielding {} elements.",
        elem_wrapper
    ));
    if clone_call.is_some() {
        builder.line("/// Each element is deep-cloned via _clone; safe past Vec close.");
    } else {
        builder.line("/// Buffer-borrowed iteration (no _clone available); don't keep yielded wrappers past the Vec's lifetime.");
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
    builder.line(&format!(
        "return object : Iterator<{}> {{",
        elem_wrapper
    ));
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
        builder.line(&format!("val __borrowed = {}(__ev.pointer)", elem_wrapper));
        builder.line("__borrowed.__consume()");
        builder.line("return __borrowed");
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
            let kt = if is_az_string_owned_arg(a) {
                "kotlin.String".to_string()
            } else if is_wrapper_class_owned_arg(a, ir) {
                kotlin_class_name(a.type_name.trim())
            } else {
                match a.ref_kind {
                    ArgRefKind::Owned => map_kt_owned(&a.type_name, ir),
                    ArgRefKind::Ref
                    | ArgRefKind::RefMut
                    | ArgRefKind::Ptr
                    | ArgRefKind::PtrMut => "Pointer?".to_string(),
                }
            };
            format!("{}: {}", sanitize_kt_identifier(&a.name), kt)
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
            if is_az_string_owned_arg(a) {
                emit_kt_az_string_conv(&mut pre_call_lines, &raw_name)
            } else if is_wrapper_class_owned_arg(a, ir) {
                let local =
                    emit_kt_wrapper_class_conv(&mut pre_call_lines, &raw_name, a.type_name.trim());
                // C ABI consumes the by-value struct; mark the
                // caller's wrapper so its deferred finalizer skips
                // AzX_delete.
                consume_after_call.push(raw_name);
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
            .filter(|r| has_kt_wrapper_class(r, ir))
            .map(|r| kotlin_class_name(r))
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
        "{}.INSTANCE.{}({})",
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
        builder.line(&format!("{}", call));
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
            let kt = if is_az_string_owned_arg(a) {
                "kotlin.String".to_string()
            } else if is_wrapper_class_owned_arg(a, ir) {
                kotlin_class_name(a.type_name.trim())
            } else {
                match a.ref_kind {
                    ArgRefKind::Owned => map_kt_owned(&a.type_name, ir),
                    ArgRefKind::Ref
                    | ArgRefKind::RefMut
                    | ArgRefKind::Ptr
                    | ArgRefKind::PtrMut => "Pointer?".to_string(),
                }
            };
            format!("{}: {}", sanitize_kt_identifier(&a.name), kt)
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
        // DeepCopy / consuming-self method.
        consume_after_call.push("this".to_string());
        "__self".to_string()
    } else {
        "this.ptr".to_string()
    };

    let mut call_args: Vec<String> = vec![self_arg];
    // Auto-string-conversion + auto-wrapper-class conversion: Owned
    // `String` args take a kotlin.String + AzString_fromUtf8 splice;
    // Owned wrapper-class args take the wrapper instance + Structure.
    // newInstance splice. Pure type-driven (see top-of-file predicates).
    for a in &user_args {
        let raw_name = sanitize_kt_identifier(&a.name);
        if is_az_string_owned_arg(a) {
            let az_name = emit_kt_az_string_conv(&mut pre_call_lines, &raw_name);
            call_args.push(az_name);
        } else if is_wrapper_class_owned_arg(a, ir) {
            let raw_local =
                emit_kt_wrapper_class_conv(&mut pre_call_lines, &raw_name, a.type_name.trim());
            consume_after_call.push(raw_name);
            call_args.push(raw_local);
        } else {
            call_args.push(raw_name);
        }
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
            .filter(|r| has_kt_wrapper_class(r, ir))
            .map(|r| kotlin_class_name(r))
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
        "{}.INSTANCE.{}({})",
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
        builder.line(&format!("{}", call));
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
}

fn emit_union_helper(builder: &mut CodeBuilder, e: &EnumDef) {
    let class_name = kotlin_class_name(&e.name);
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
                let mname = idiomatic_method_name(&v.name);
                let variant_ident = sanitize_kt_identifier(&v.name);
                builder.line(&format!(
                    "/** Construct the {}.{} variant. */",
                    e.name, v.name
                ));
                builder.line(&format!(
                    "@JvmStatic fun {}(): {} {{",
                    mname, ffi_name
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
                builder.line(&format!(
                    "// SKIPPED: variant {}.{} carries a payload — set the variant",
                    e.name, v.name
                ));
                builder.line(&format!(
                    "// via {0}.{1} fields directly (see {0}Variant_{1}).",
                    ffi_name, v.name
                ));
                builder.blank();
            }
        }
    }

    builder.dedent();
    builder.line("}");
    builder.blank();
}

fn idiomatic_method_name(method_name: &str) -> String {
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
/// - `*/` inside paths like `/users/*/name` is read as the doc-comment
///   terminator, prematurely closing the KDoc and surfacing as
///   "Missing '}" / "Unclosed comment" errors on later lines.
/// - `{` / `}` are KDoc inline-tag delimiters. Unbalanced braces from
///   inline code samples (`r#"{"users":...}"#`) trip the doc parser.
pub(crate) fn kdoc_escape(s: &str) -> String {
    s.replace("*/", "*&#47;")
        .replace('{', "&#123;")
        .replace('}', "&#125;")
}

