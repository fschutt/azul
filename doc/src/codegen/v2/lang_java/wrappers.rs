//! AutoCloseable wrapper-class emission for the Java JNA generator.
//!
//! For every IR struct that has a matching `<TypeName>_delete` C
//! function we emit a `public final class <TypeName> implements
//! AutoCloseable` that:
//!
//! - Holds the underlying JNA `Pointer` in a private field
//! - Provides `close()` calling `AzulNative.INSTANCE.Az<Type>_delete(ptr)`
//! - Surfaces every non-trait method on the IR class as either an
//!   instance method (`fn(self, ...)`) or a `public static` factory
//!   (`fn() -> Self`)
//!
//! Tagged-union enums get a separate, very minimal helper class with
//! static factories per unit variant. (Payload-bearing variants are
//! left for the user to construct via the generated `Az<Type>` Union
//! plus the matching `<Type>Variant_<Variant>` payload struct — see
//! `types.rs`.)

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{
    ArgRefKind, CodegenIR, EnumDef, EnumVariantKind, FieldRefKind, FunctionArg, FunctionDef,
    FunctionKind, MonomorphizedKind, StructDef, TypeCategory,
};
use super::types::{java_boxed, ref_kind_field_type};
use super::{emit_file, ffi_type_name, map_jvm_type, map_jvm_type_byvalue, sanitize_identifier, snake_to_lower_camel};

// ============================================================================
// Top-level driver
// ============================================================================

pub fn emit_all_wrapper_files(
    out: &mut String,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    for s in &ir.structs {
        if !should_emit_wrapper(s, ir, config) {
            continue;
        }
        let class_name = wrapper_class_name(&s.name);
        let chunk = emit_file(
            &format!("{}.java", class_name),
            |b| {
                emit_wrapper_class(b, s, ir);
                Ok(())
            },
            config,
        )?;
        out.push_str(&chunk);
    }

    for e in &ir.enums {
        if !should_emit_union_helper(e, config) {
            continue;
        }
        let helper_name = format!("{}Helpers", wrapper_class_name(&e.name));
        let chunk = emit_file(
            &format!("{}.java", helper_name),
            |b| {
                emit_union_helper(b, e);
                Ok(())
            },
            config,
        )?;
        out.push_str(&chunk);
    }

    Ok(())
}

// ============================================================================
// Filters
// ============================================================================

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
    has_delete_function(&s.name, ir)
}

fn should_emit_union_helper(e: &EnumDef, config: &CodegenConfig) -> bool {
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

fn has_delete_function(type_name: &str, ir: &CodegenIR) -> bool {
    ir.functions
        .iter()
        .any(|f| f.class_name == type_name && f.kind == FunctionKind::Delete)
}

/// True iff the codegen emits a `class <type_name> extends AutoCloseable`
/// wrapper for this type — i.e. it's a non-excluded struct with a
/// `_delete` C function. Enums (e.g. `CssDeclaration`,
/// `AccessibilityAction`) get only a `<X>Helpers` static-factory class
/// and an `Az<X>` JNA Union; no constructor-taking-Pointer is available
/// and `new <X>(...)` would be a compile error.
fn has_wrapper_class(type_name: &str, ir: &CodegenIR) -> bool {
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
    has_delete_function(type_name, ir)
}

// ============================================================================
// Wrapper emission
// ============================================================================

fn emit_wrapper_class(builder: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR) {
    let class_name = wrapper_class_name(&s.name);
    let ffi_name = ffi_type_name(&s.name);

    if !s.doc.is_empty() {
        builder.line("/**");
        for d in &s.doc {
            builder.line(&format!(" * {}", javadoc_escape(d)));
        }
        builder.line(" */");
    }

    // Phase I.1.2 (Java): if this wrapper's underlying struct matches
    // the Vec shape (ptr/len/cap/destructor), declare `implements
    // Iterable<T>` so user code can write `for (T x : vec)`.
    let vec_elem_type = detect_vec_elem_type_jvm(s);
    // Iterable only when the element type is an emitted struct
    // wrapper class — skip enum/typedef elements (`IdOrClass`,
    // `DynamicSelector`, etc.) which don't get their own class.
    let elem_has_wrapper = |elem: &str| -> bool {
        ir.find_struct(elem).is_some() && has_delete_function(elem, ir)
    };
    let interfaces = match &vec_elem_type {
        Some(elem) if elem_has_wrapper(elem) => {
            let elem_wrapper = wrapper_class_name(elem);
            format!("AutoCloseable, Iterable<{}>", elem_wrapper)
        }
        _ => "AutoCloseable".to_string(),
    };
    builder.line(&format!(
        "public final class {} implements {} {{",
        class_name, interfaces
    ));
    builder.indent();

    builder.line("private Pointer ptr;");
    builder.line("private boolean closed;");
    builder.blank();

    // Internal pointer-wrapping constructor (package-private).
    builder.line(&format!(
        "/** Wrap an existing native {} pointer; takes ownership. */",
        ffi_name
    ));
    builder.line(&format!(
        "{}(Pointer ptr) {{ this.ptr = ptr; this.closed = false; }}",
        class_name
    ));
    builder.blank();

    builder.line("/** Internal: raw pointer for use by sibling wrappers. */");
    builder.line("public Pointer rawPointer() { return ptr; }");
    builder.blank();

    // AzString gets a `toString()` override that decodes the wrapped
    // UTF-8 bytes into a `java.lang.String`. AzString's C-side layout
    // is `{ vec: AzU8Vec }`, and AzU8Vec is
    // `{ ptr: u8*, len: usize, cap: usize, destructor: AzU8VecDestructor }`.
    // The wrapper's `ptr` is the address of the AzString struct, so
    // offset 0 is `vec.ptr` (the UTF-8 byte buffer) and offset 8 is
    // `vec.len` (byte length).
    if matches!(s.category, TypeCategory::String) {
        builder.line("/**");
        builder.line(" * Decode the wrapped UTF-8 bytes into a `java.lang.String`.");
        builder.line(" * Reads `vec.ptr` (offset 0) and `vec.len` (offset 8) from");
        builder.line(" * the AzString struct directly via JNA.");
        builder.line(" */");
        builder.line("@Override");
        builder.line("public java.lang.String toString() {");
        builder.indent();
        builder.line("if (ptr == null || closed) return \"\";");
        builder.line("Pointer vecPtr = ptr.getPointer(0);");
        builder.line("long vecLen = ptr.getLong(8);");
        builder.line("if (vecPtr == null || vecLen <= 0) return \"\";");
        builder.line("byte[] bytes = vecPtr.getByteArray(0, (int) vecLen);");
        builder.line("return new java.lang.String(bytes, java.nio.charset.StandardCharsets.UTF_8);");
        builder.dedent();
        builder.line("}");
        builder.blank();
    }

    // Phase J.1: generalize the Button.onClick hardcode to any widget
    // method with the `with_on_*(self, data: RefAny, callback: <Cb>)`
    // shape. The detector (`smart_callback_setter_info`) returns
    // Some((smart_name, wrapper_name)) when the method matches the
    // pattern AND the wrapper kind is in HOST_INVOKER_KINDS.
    //
    // This lights up CheckBox.onToggle(data, fn), TextInput.onTextInput(),
    // DropDown.onChoiceChange(), and friends — every widget that has a
    // with_on_<event>(refany, callback) builder gets an idiomatic
    // sibling that wraps both internally.
    for func in ir.functions_for_class(&s.name) {
        let Some((smart_snake, wrapper_kind)) =
            super::super::managed_host_invoker::smart_callback_setter_info(func)
        else {
            continue;
        };
        let smart_camel = snake_to_lower_camel(&smart_snake);
        let with_camel = idiomatic_method_name(&func.method_name);
        let sam_class = format!("AzulNativeManaged.{}InvokerCallback", wrapper_kind);
        let register_method = if wrapper_kind == "Callback" {
            "registerCallback".to_string()
        } else {
            format!("register{}", wrapper_kind)
        };
        builder.line("/**");
        builder.line(&format!(
            " * Smart builder for {}: takes a Java object as data and a",
            with_camel
        ));
        builder.line(" * SAM callback; host-invoker registration of both happens");
        builder.line(" * internally.");
        builder.line(" */");
        builder.line(&format!(
            "public {} {}(Object data, {} fn) {{",
            class_name, smart_camel, sam_class
        ));
        builder.indent();
        builder.line("AzRefAny.ByValue __data = AzulHostInvoker.refanyCreate(data);");
        builder.line(&format!(
            "Az{}.ByValue __cb = AzulHostInvoker.{}(fn);",
            wrapper_kind, register_method
        ));
        builder.line(&format!(
            "return {}(new RefAny(__data.getPointer()), new {}(__cb.getPointer()));",
            with_camel, wrapper_kind
        ));
        builder.dedent();
        builder.line("}");
        builder.blank();
    }

    // WindowCreateOptions.create(LayoutCallbackInvokerCallback) — smart
    // factory that hides the host-invoker plumbing. The user passes a
    // SAM callback; we register it via AzulHostInvoker, splice the
    // resulting AzLayoutCallback bytes into a `_default()` WCO's
    // embedded layout_callback storage, and hand back the wrapped
    // instance. Replaces the manual:
    //
    //     AzLayoutCallback.ByValue cb = AzulHostInvoker.registerLayoutCallback(fn);
    //     AzWindowCreateOptions.ByValue wco = AzulNativeWindow.AzWindowCreateOptions_default();
    //     cb.write(); wco.write();
    //     wco.window_state.layout_callback.getPointer().write(0, cb.getPointer().getByteArray(0, cb.size()), 0, cb.size());
    //     wco.read();
    //
    // boilerplate every JVM hello-world has today.
    if let Some(info) = super::super::managed_host_invoker::layout_callback_factory_info(s, ir) {
        let wrapper_class = wrapper_class_name(&info.class_name);
        let ffi_class = ffi_type_name(&info.class_name);
        let cb_ffi = ffi_type_name(&info.callback_wrapper);
        let register_fn = format!("register{}", info.callback_wrapper);
        let native_class = super::functions::native_class_for_class(&info.class_name, ir);
        let field_path = info.field_path.join(".");
        let sam_raw = format!(
            "AzulNativeManaged.{}InvokerCallback",
            info.callback_wrapper
        );
        let sam_typed = format!("AzulHostInvoker.{}", info.callback_wrapper);

        // Emit two overloads — raw (4-arg outPtr-write) and typed
        // (returns wrapper struct directly). Both bodies are derived
        // from the same factory info; differ only in the SAM type.
        for (sam_type, doc_note) in [
            (
                sam_raw.as_str(),
                "Smart factory: pass a layout-callback lambda; the host-invoker registration and bytes-copy plumbing happen internally.",
            ),
            (
                sam_typed.as_str(),
                "Smart factory (typed): pass a typed callback that returns a wrapper struct directly; the bridge splices the bytes into the embedded callback field.",
            ),
        ] {
            builder.line("/**");
            builder.line(&format!(" * {}", doc_note));
            builder.line(" */");
            builder.line(&format!(
                "public static {} create({} fn) {{",
                wrapper_class, sam_type
            ));
            builder.indent();
            builder.line(&format!(
                "{}.ByValue __cb = AzulHostInvoker.{}(fn);",
                cb_ffi, register_fn
            ));
            builder.line(&format!(
                "{}.ByValue __wco = {}.INSTANCE.{}();",
                ffi_class, native_class, info.default_c_name
            ));
            builder.line("__cb.write();");
            builder.line("__wco.write();");
            builder.line("byte[] __cbBytes = __cb.getPointer().getByteArray(0, __cb.size());");
            builder.line(&format!(
                "__wco.{}.getPointer().write(0, __cbBytes, 0, __cbBytes.length);",
                field_path
            ));
            builder.line("__wco.read();");
            builder.line(&format!(
                "return new {}(__wco.getPointer());",
                wrapper_class
            ));
            builder.dedent();
            builder.line("}");
            builder.blank();
        }
    }

    // Methods.
    for func in ir.functions_for_class(&s.name) {
        if func.kind.is_trait_function() {
            continue;
        }
        emit_wrapper_method(builder, &class_name, func, ir);
    }

    // Phase I.2: route Object.equals(Object) + hashCode() through the
    // codegen-emitted `_partialEq` / `_hash` C-ABI helpers when
    // TypeTraits says they're supported. Pure type-driven; falls back
    // to identity-based defaults when the helpers aren't available.
    emit_equals_hashcode_if_supported(builder, s, &class_name, ir);

    // Phase I.3 (Java): override Object.toString() through the
    // codegen-emitted `Az<X>_toDbgString` C-ABI helper when TypeTraits
    // flags `is_debug`. Existing AzString toString override is left in
    // place since it accesses the underlying U8Vec directly (no helper
    // round-trip).
    emit_toString_if_supported(builder, s, ir);

    // Phase I.1.2 (Java): emit Iterable<T>.iterator() when the Vec
    // shape was detected AND the element has a wrapper class. The
    // body overlays AzXVec via JNA Structure.newInstance, reads
    // ptr+len, and walks the buffer one element at a time.
    if let Some(elem) = vec_elem_type.as_deref() {
        if ir.find_struct(elem).is_some() && has_delete_function(elem, ir) {
            emit_jvm_vec_iterator(builder, s, elem);
        }
    }

    // close() / AutoCloseable.
    emit_close_method(builder, &s.name, &class_name, ir);

    builder.dedent();
    builder.line("}");
}

/// Phase I.2 (Java): override Object.equals(Object) + hashCode() to
/// route through the codegen-emitted `Az<X>_partialEq` / `Az<X>_hash`
/// C exports. Pure type-driven from `TypeTraits.is_partial_eq` /
/// `TypeTraits.is_hash`; only emits the override when the helper
/// actually exists in `ir.functions`.
fn emit_equals_hashcode_if_supported(
    builder: &mut CodeBuilder,
    s: &StructDef,
    class_name: &str,
    ir: &CodegenIR,
) {
    let native = super::functions::native_class_for_class(&s.name, ir);
    let eq_sym = format!("Az{}_partialEq", s.name);
    let has_eq = s.traits.is_partial_eq
        && ir.functions.iter().any(|f| f.c_name == eq_sym);
    let hash_sym = format!("Az{}_hash", s.name);
    let has_hash = s.traits.is_hash
        && ir.functions.iter().any(|f| f.c_name == hash_sym);

    if has_eq {
        builder.line("/**");
        builder.line(" * Equality routed through the codegen-emitted");
        builder.line(&format!(" * {} C-ABI helper.", eq_sym));
        builder.line(" */");
        builder.line("@Override");
        builder.line("public boolean equals(Object other) {");
        builder.indent();
        builder.line(&format!("if (!(other instanceof {})) return false;", class_name));
        builder.line(&format!("{} o = ({}) other;", class_name, class_name));
        builder.line("if (this.ptr == null || o.ptr == null) return this.ptr == o.ptr;");
        // JNA maps C `bool` to `byte` on macOS/Linux (no explicit
        // @MarshalAs(U1)). Compare against zero.
        builder.line(&format!(
            "return {}.INSTANCE.{}(this.ptr, o.ptr) != 0;",
            native, eq_sym
        ));
        builder.dedent();
        builder.line("}");
        builder.blank();
    }

    if has_hash {
        builder.line("/**");
        builder.line(" * Hash routed through the codegen-emitted");
        builder.line(&format!(" * {} C-ABI helper.", hash_sym));
        builder.line(" */");
        builder.line("@Override");
        builder.line("public int hashCode() {");
        builder.indent();
        builder.line("if (ptr == null) return 0;");
        builder.line(&format!(
            "long h = {}.INSTANCE.{}(ptr);",
            native, hash_sym
        ));
        builder.line("return (int) (h ^ (h >>> 32));");
        builder.dedent();
        builder.line("}");
        builder.blank();
    } else if has_eq {
        // Contract requires hashCode override when equals is overridden.
        // Fall back to a Pointer-based hash so the contract holds.
        builder.line("/** Identity-based hashCode to honor the equals/hashCode contract. */");
        builder.line("@Override");
        builder.line("public int hashCode() {");
        builder.indent();
        builder.line("return ptr == null ? 0 : ptr.hashCode();");
        builder.dedent();
        builder.line("}");
        builder.blank();
    }
}

/// Phase I.3 (Java): override Object.toString() routed through the
/// codegen-emitted `Az<X>_toDbgString` C export when TypeTraits.is_debug
/// is set and the helper actually exists. Skips when this is the String
/// wrapper class — that already has a vec-direct toString.
fn emit_toString_if_supported(
    builder: &mut CodeBuilder,
    s: &StructDef,
    ir: &CodegenIR,
) {
    if matches!(s.category, TypeCategory::String) {
        return;
    }
    let dbg_sym = format!("Az{}_toDbgString", s.name);
    let has_dbg = s.traits.is_debug
        && ir.functions.iter().any(|f| f.c_name == dbg_sym);
    if !has_dbg {
        return;
    }
    let native = super::functions::native_class_for_class(&s.name, ir);
    builder.line("/**");
    builder.line(&format!(
        " * String representation routed through {}.",
        dbg_sym
    ));
    builder.line(" */");
    builder.line("@Override");
    builder.line("public java.lang.String toString() {");
    builder.indent();
    builder.line("if (ptr == null || closed) return super.toString();");
    builder.line(&format!(
        "AzString.ByValue __s = {}.INSTANCE.{}(ptr);",
        native, dbg_sym
    ));
    // Decode the AzString. The AzString struct's first field is a
    // U8Vec; offset 0 is vec.ptr, offset 8 is vec.len.
    builder.line("__s.write();");
    builder.line("Pointer __sp = __s.getPointer();");
    builder.line("Pointer __vecPtr = __sp.getPointer(0);");
    builder.line("long __vecLen = __sp.getLong(8);");
    builder.line("if (__vecPtr == null || __vecLen <= 0) return \"\";");
    builder.line("byte[] __bytes = __vecPtr.getByteArray(0, (int) __vecLen);");
    builder.line("java.lang.String __out = new java.lang.String(__bytes, java.nio.charset.StandardCharsets.UTF_8);");
    // Free the freshly-allocated AzString to avoid leaking the U8Vec.
    builder.line("AzulNativeStr.INSTANCE.AzString_delete(__sp);");
    builder.line("return __out;");
    builder.dedent();
    builder.line("}");
    builder.blank();
}

// Phase J.1 detector now lives in `codegen::v2::managed_host_invoker`
// as `smart_callback_setter_info` — shared across every binding.

/// Phase I.1.2 (Java): Vec-shape detector. Mirrors the Haskell H.3 /
/// Ruby I.1.6 pattern: struct fields exactly [ptr, len, cap, destructor]
/// with ptr being a `*mut|*const T` typedef. Returns the element type T.
fn detect_vec_elem_type_jvm(s: &StructDef) -> Option<String> {
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

/// Emit `Iterable<T>.iterator()` body for a Vec wrapper. Reads
/// ptr/len from the underlying AzXVec struct overlay, then yields
/// wrapper-class instances by overlaying each element at
/// `ptr + i * elemSize`.
fn emit_jvm_vec_iterator(
    builder: &mut CodeBuilder,
    s: &StructDef,
    elem_type: &str,
) {
    let vec_ffi = ffi_type_name(&s.name);
    let elem_ffi = ffi_type_name(elem_type);
    let elem_wrapper = wrapper_class_name(elem_type);
    builder.line("/**");
    builder.line(&format!(
        " * Phase I.1: iterate the underlying Vec yielding {} elements.",
        elem_wrapper
    ));
    builder.line(" * Each element overlays a slice of the native buffer via JNA.");
    builder.line(" */");
    builder.line("@Override");
    builder.line(&format!(
        "public java.util.Iterator<{}> iterator() {{",
        elem_wrapper
    ));
    builder.indent();
    builder.line(&format!(
        "final {}.ByValue __raw = (({}.ByValue) Structure.newInstance({}.ByValue.class, ptr));",
        vec_ffi, vec_ffi, vec_ffi
    ));
    builder.line("__raw.read();");
    builder.line("final Pointer __buf = __raw.ptr;");
    builder.line("final long __n = __raw.len;");
    builder.line(&format!(
        "final int __sz = Structure.newInstance({}.class).size();",
        elem_ffi
    ));
    builder.line(&format!(
        "return new java.util.Iterator<{}>() {{",
        elem_wrapper
    ));
    builder.indent();
    builder.line("private long __i = 0;");
    builder.line("@Override public boolean hasNext() { return __i < __n; }");
    builder.line("@Override");
    builder.line(&format!("public {} next() {{", elem_wrapper));
    builder.indent();
    builder.line("if (__i >= __n) throw new java.util.NoSuchElementException();");
    builder.line("Pointer __ep = __buf.share(__i * __sz);");
    builder.line("__i++;");
    builder.line(&format!(
        "{}.ByValue __ev = ({}.ByValue) Structure.newInstance({}.ByValue.class, __ep);",
        elem_ffi, elem_ffi, elem_ffi
    ));
    builder.line("__ev.read();");
    builder.line(&format!("return new {}(__ev.getPointer());", elem_wrapper));
    builder.dedent();
    builder.line("}");
    builder.dedent();
    builder.line("};");
    builder.dedent();
    builder.line("}");
    builder.blank();
}

fn emit_close_method(builder: &mut CodeBuilder, raw_type_name: &str, class_name: &str, ir: &CodegenIR) {
    builder.line("/** Frees the underlying native resources. Idempotent. */");
    builder.line("@Override");
    builder.line("public void close() {");
    builder.indent();
    builder.line("if (closed || ptr == null) return;");
    builder.line(&format!(
        "{}.INSTANCE.Az{}_delete(ptr);",
        super::functions::native_class_for_class(raw_type_name, ir),
        raw_type_name
    ));
    builder.line("ptr = null;");
    builder.line("closed = true;");
    builder.dedent();
    builder.line("}");
    builder.blank();

    // Defensive finalizer in case the user forgets try-with-resources.
    builder.line("@Override");
    builder.line("@SuppressWarnings(\"deprecation\")");
    builder.line("protected void finalize() throws Throwable {");
    builder.indent();
    builder.line("try { close(); } finally { super.finalize(); }");
    builder.dedent();
    builder.line("}");
    let _ = class_name;
}

/// Phase I.5.1: how the wrapper method should idiomise an Option<T> /
/// Result<T, E> return. Detection lives in [`classify_return`]; the
/// caller computes the user-visible display type from the carried
/// `payload_ty` + `ref_kind` and rewrites the method body to call
/// `__ret.toNullable()` / `__ret.unwrap()` on the FFI struct.
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

/// Look up the wrapper-method return type and decide whether it should
/// be idiomised at the call site. Mirrors the Ruby/Node `classify_return`
/// predicate but extracts the payload type so the Java side can produce
/// a typed `java.util.Optional<T>` signature rather than a raw `Object`.
fn classify_return(func: &FunctionDef, ir: &CodegenIR) -> ReturnIdiom {
    let Some(rt) = func.return_type.as_deref() else {
        return ReturnIdiom::Plain;
    };
    let rt = rt.trim();
    // Az*Option / Az*Result types are monomorphized aliases — the
    // codegen IR stores them as `TypeAliasDef.monomorphized_def`.
    if let Some(ta) = ir.find_type_alias(rt) {
        if let Some(ref mono) = ta.monomorphized_def {
            if let MonomorphizedKind::TaggedUnion { ref variants, .. } = mono.kind {
                if variants.len() == 2 {
                    let none = variants.iter().find(|v| v.name == "None");
                    let some = variants.iter().find(|v| v.name == "Some");
                    if let (Some(_), Some(sv)) = (none, some) {
                        if let Some(ref payload_ty) = sv.payload_type {
                            return ReturnIdiom::Option {
                                payload_ty: payload_ty.clone(),
                                ref_kind: sv.payload_ref_kind.clone(),
                            };
                        }
                    }
                    let ok = variants.iter().find(|v| v.name == "Ok");
                    let err = variants.iter().find(|v| v.name == "Err");
                    if let (Some(ov), Some(_)) = (ok, err) {
                        if let Some(ref payload_ty) = ov.payload_type {
                            return ReturnIdiom::Result {
                                payload_ty: payload_ty.clone(),
                                ref_kind: ov.payload_ref_kind.clone(),
                            };
                        }
                    }
                }
            }
        }
    }
    // Fallback: hand-authored Option/Result enums (rare; the api.json
    // sources are normally typedefs).
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

/// Map a payload's "raw" Java field type (what JNA carries on the
/// FFI struct) to the user-visible display type at the wrapper
/// boundary:
///
/// - `AzString` → `java.lang.String` (UTF-8 decode inline)
/// - `AzX` with a wrapper class → `X` (the wrapper)
/// - Anything else → the raw type itself (primitives stay primitives;
///   `Pointer` stays `Pointer`; raw FFI structs without a wrapper stay
///   `AzY`)
fn payload_display_type(raw: &str, ir: &CodegenIR) -> String {
    if let Some(unprefixed) = raw.strip_prefix("Az") {
        // TypeCategory-driven (J.3 pattern): any struct flagged as
        // String in api.json — not just the literal "String" type —
        // gets the java.lang.String round-trip at the wrapper boundary.
        if let Some(s) = ir.find_struct(unprefixed) {
            if matches!(s.category, TypeCategory::String) {
                return "java.lang.String".to_string();
            }
        }
        if has_wrapper_class(unprefixed, ir) {
            return unprefixed.to_string();
        }
    }
    raw.to_string()
}

/// Detect whether a payload's raw Java type maps to an
/// `azul.json`-categorised String struct (i.e. UTF-8-decode at the
/// boundary applies).
fn is_az_string_jvm(raw: &str, ir: &CodegenIR) -> bool {
    let Some(unprefixed) = raw.strip_prefix("Az") else {
        return false;
    };
    ir.find_struct(unprefixed)
        .map(|s| matches!(s.category, TypeCategory::String))
        .unwrap_or(false)
}

fn emit_wrapper_method(
    builder: &mut CodeBuilder,
    class_name: &str,
    func: &FunctionDef,
    ir: &CodegenIR,
) {
    let method_name = idiomatic_method_name(&func.method_name);
    let _ = ffi_type_name(&func.class_name);

    let return_jvm = func
        .return_type
        .as_ref()
        .map(|r| map_jvm_type_byvalue(r, ir))
        .unwrap_or_else(|| "void".to_string());

    let idiom = classify_return(func, ir);

    let takes_self = matches!(
        func.kind,
        FunctionKind::Method | FunctionKind::MethodMut | FunctionKind::DeepCopy
    );

    // For takes_self methods the first arg in `func.args` IS the
    // implicit self, regardless of the name the api.json gave it
    // (`instance`, lowercased class name, etc.). Skip args[0] unconditionally
    // when takes_self; otherwise filter by conventional self names so a
    // legitimate user arg named after the class still passes through.
    let user_args: Vec<_> = if takes_self {
        func.args.iter().skip(1).collect()
    } else {
        let class_lower = func.class_name.to_lowercase();
        func.args
            .iter()
            .filter(|a| a.name != class_lower && a.name != "self")
            .collect()
    };

    // Auto-conversion rules (both type-driven; no method-name allow-
    // list, no per-class hardcoding):
    //
    // 1. AzString Owned: parameter takes `java.lang.String`; emit a
    //    UTF-8-bytes → AzString_fromUtf8 conversion pre-call line.
    // 2. Wrapper-class Owned: parameter takes the wrapper class (e.g.
    //    `Dom child` instead of `AzDom.ByValue child`); emit a
    //    Structure.newInstance + .read() splice pre-call line.
    //
    // Both apply uniformly to every emitted wrapper method.
    let is_az_string_owned_arg = |a: &&FunctionArg| -> bool {
        a.type_name.trim() == "String" && matches!(a.ref_kind, ArgRefKind::Owned)
    };
    let is_wrapper_class_owned_arg = |a: &&FunctionArg| -> bool {
        if !matches!(a.ref_kind, ArgRefKind::Owned) {
            return false;
        }
        let tn = a.type_name.trim();
        if tn == "String" {
            return false;
        }
        // Strict: only treat as wrapper-class arg if the codegen
        // actually emits a wrapper file for it (i.e. has a delete fn
        // and isn't in an excluded TypeCategory). Without this guard,
        // structs that exist in the IR but never get a wrapper class
        // (Vec inner types, internal data carriers) get over-converted
        // and the generated code references missing classes.
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
        has_delete_function(tn, ir)
    };

    let arg_sig: Vec<String> = user_args
        .iter()
        .map(|a| {
            let jt = if is_az_string_owned_arg(a) {
                "java.lang.String".to_string()
            } else if is_wrapper_class_owned_arg(a) {
                // Wrapper class — strip `Az` prefix the same way
                // `wrapper_class_name` would.
                wrapper_class_name(a.type_name.trim())
            } else {
                match a.ref_kind {
                    ArgRefKind::Owned => map_jvm_type_byvalue(&a.type_name, ir),
                    ArgRefKind::Ref
                    | ArgRefKind::RefMut
                    | ArgRefKind::Ptr
                    | ArgRefKind::PtrMut => "Pointer".to_string(),
                }
            };
            format!("{} {}", jt, sanitize_identifier(&a.name))
        })
        .collect();

    let is_static = matches!(
        func.kind,
        FunctionKind::Constructor | FunctionKind::StaticMethod | FunctionKind::Default
    );

    // Some C ABIs take self by VALUE (e.g. `AzRibbon_renderDom(AzRibbon r)`)
    // rather than by pointer (`AzFoo_*(IntPtr instance, ...)`). Detect via
    // the first arg's ref_kind (Owned = by value). When self-by-value, we
    // construct a `Az<Type>.ByValue` whose Pointer points at our heap-held
    // instance and pass it in.
    let self_by_value = takes_self
        && func
            .args
            .first()
            .map(|a| matches!(a.ref_kind, ArgRefKind::Owned))
            .unwrap_or(false);

    let mut pre_call_lines: Vec<String> = Vec::new();
    let mut call_args: Vec<String> = Vec::new();
    if takes_self {
        if self_by_value {
            // Build a JNA `.ByValue` Structure overlaying our pointer
            // via the public Structure.newInstance(Class, Pointer)
            // factory. `useMemory` is protected and not callable from
            // here; `newInstance` is the canonical replacement.
            let self_ty = ffi_type_name(&func.class_name);
            pre_call_lines.push(format!(
                "{}.ByValue __self = Structure.newInstance({}.ByValue.class, this.ptr);",
                self_ty, self_ty
            ));
            pre_call_lines.push("__self.read();".to_string());
            call_args.push("__self".to_string());
        } else {
            call_args.push("this.ptr".to_string());
        }
    }
    // Callback args: do NOT auto-substitute at the wrapper-method
    // boundary. The wrapper signature carries the C ABI type (e.g.
    // `AzCallback.ByValue` or `AzCallbackType`) and is passed through
    // unchanged. Users construct the wrapper struct via
    // `AzulHostInvoker.register*(handler)` themselves and pass that.
    // (Same conclusion C# / Lua reached.)
    //
    // Auto-string-conversion: any Owned `String` arg accepts a
    // `java.lang.String` at the wrapper level. Convert UTF-8 bytes →
    // AzString.ByValue via the C-API helper before the call.
    for a in &user_args {
        let raw_name = sanitize_identifier(&a.name);
        if is_az_string_owned_arg(a) {
            let az_name = format!("__{}_az", raw_name);
            let bytes_name = format!("__{}_bytes", raw_name);
            let mem_name = format!("__{}_mem", raw_name);
            pre_call_lines.push(format!(
                "byte[] {bytes} = {raw}.getBytes(java.nio.charset.StandardCharsets.UTF_8);",
                bytes = bytes_name,
                raw = raw_name,
            ));
            pre_call_lines.push(format!(
                "com.sun.jna.Memory {mem} = new com.sun.jna.Memory({bytes}.length);",
                mem = mem_name,
                bytes = bytes_name,
            ));
            pre_call_lines.push(format!(
                "{mem}.write(0, {bytes}, 0, {bytes}.length);",
                mem = mem_name,
                bytes = bytes_name,
            ));
            pre_call_lines.push(format!(
                "AzString.ByValue {az} = AzulNativeStr.INSTANCE.AzString_fromUtf8({mem}, {bytes}.length);",
                az = az_name,
                mem = mem_name,
                bytes = bytes_name,
            ));
            call_args.push(az_name);
        } else if is_wrapper_class_owned_arg(a) {
            // Splice the wrapper's underlying Pointer into a
            // by-value Structure overlay so the C ABI sees a real
            // struct value. Same pattern the self-by-value path uses.
            let ffi = ffi_type_name(a.type_name.trim());
            let raw_local = format!("__{}_raw", raw_name);
            pre_call_lines.push(format!(
                "{ffi}.ByValue {raw_local} = Structure.newInstance({ffi}.ByValue.class, {arg}.rawPointer());",
                ffi = ffi,
                raw_local = raw_local,
                arg = raw_name,
            ));
            pre_call_lines.push(format!("{}.read();", raw_local));
            call_args.push(raw_local);
        } else {
            call_args.push(raw_name);
        }
    }

    if !func.doc.is_empty() {
        builder.line("/**");
        for d in &func.doc {
            builder.line(&format!(" * {}", javadoc_escape(d)));
        }
        builder.line(" */");
    }

    let returns_self = func
        .return_type
        .as_deref()
        .map(|r| r.trim() == func.class_name)
        .unwrap_or(false);

    // Phase I.5.1: idiomise Option<T> / Result<T, E> return types at the
    // wrapper boundary. The FFI struct still exposes
    // `toNullable()` / `unwrap()` (from `types.rs`); the wrapper layer
    // simply rebrands the visible signature to `java.util.Optional<T>`
    // for Option and the bare payload type for Result (which throws on
    // Err — same idiom as Rust's `Result::unwrap`).
    let displayed_return = if returns_self {
        class_name.to_string()
    } else {
        match &idiom {
            ReturnIdiom::Plain => return_jvm.clone(),
            ReturnIdiom::Option {
                payload_ty,
                ref_kind,
            } => {
                let raw = ref_kind_field_type(payload_ty, ref_kind, ir);
                let display = payload_display_type(&raw, ir);
                format!("java.util.Optional<{}>", java_boxed(&display))
            }
            ReturnIdiom::Result {
                payload_ty,
                ref_kind,
            } => {
                let raw = ref_kind_field_type(payload_ty, ref_kind, ir);
                let display = payload_display_type(&raw, ir);
                java_boxed(&display)
            }
        }
    };

    let modifiers = if is_static { "public static" } else { "public" };

    builder.line(&format!(
        "{} {} {}({}) {{",
        modifiers,
        displayed_return,
        method_name,
        arg_sig.join(", ")
    ));
    builder.indent();

    if !is_static {
        builder.line("if (closed) throw new IllegalStateException(\"closed\");");
    }

    for stmt in &pre_call_lines {
        builder.line(stmt);
    }

    // Use `func.c_name` directly — it is already the camelCase native
    // symbol name (`AzFoo_withCapacity`) that matches the AzulNative
    // interface declarations. Reconstructing it from method_name yields
    // snake_case (`AzFoo_with_capacity`) which drifts from the actual
    // C ABI symbol the codegen registered.
    let call = format!(
        "{}.INSTANCE.{}({})",
        super::functions::native_class_for_func(func, ir),
        func.c_name,
        call_args.join(", ")
    );

    if return_jvm == "void" {
        builder.line(&format!("{};", call));
    } else if returns_self {
        // The C ABI returned a struct-by-value; the JNA shim returned a
        // ByValue Structure. We adopt its `Pointer` for the wrapper.
        builder.line(&format!("{} __raw = {};", return_jvm, call));
        builder.line(&format!(
            "return new {}(__raw.getPointer());",
            class_name
        ));
    } else {
        match &idiom {
            ReturnIdiom::Option {
                payload_ty,
                ref_kind,
            } => {
                let raw = ref_kind_field_type(payload_ty, ref_kind, ir);
                builder.line(&format!("{} __ret = {};", return_jvm, call));
                emit_option_return_body(builder, &raw, ir);
            }
            ReturnIdiom::Result {
                payload_ty,
                ref_kind,
            } => {
                let raw = ref_kind_field_type(payload_ty, ref_kind, ir);
                builder.line(&format!("{} __ret = {};", return_jvm, call));
                emit_result_return_body(builder, &raw, ir);
            }
            ReturnIdiom::Plain => {
                builder.line(&format!("return {};", call));
            }
        }
    }

    builder.dedent();
    builder.line("}");
    builder.blank();
}

// ============================================================================
// Phase I.5.1 — Option<T> / Result<T, E> idiomatic return bodies
// ============================================================================

/// Emit the body of a wrapper method whose return is `Optional<T>`. The
/// FFI `AzOption*.ByValue __ret` has already been declared; we call
/// `__ret.toNullable()` and wrap the result for the host idiom.
///
/// Three paths based on the raw FFI payload type:
/// 1. `AzString` — decode UTF-8 bytes into `java.lang.String` inline.
/// 2. `AzX` with a wrapper class — construct `new X(__nv.getPointer())`.
/// 3. Anything else (primitives, raw FFI structs without wrappers,
///    `Pointer`) — return `Optional.ofNullable(__ret.toNullable())`
///    directly.
fn emit_option_return_body(builder: &mut CodeBuilder, raw_payload_jvm: &str, ir: &CodegenIR) {
    if is_az_string_jvm(raw_payload_jvm, ir) {
        builder.line(&format!("{} __nv = __ret.toNullable();", raw_payload_jvm));
        builder.line("if (__nv == null) return java.util.Optional.empty();");
        builder.line("Pointer __sp = __nv.getPointer();");
        builder.line("Pointer __vecPtr = __sp.getPointer(0);");
        builder.line("long __vecLen = __sp.getLong(8);");
        builder.line("if (__vecPtr == null || __vecLen <= 0) return java.util.Optional.of(\"\");");
        builder.line("byte[] __bytes = __vecPtr.getByteArray(0, (int) __vecLen);");
        builder.line(
            "return java.util.Optional.of(new java.lang.String(__bytes, java.nio.charset.StandardCharsets.UTF_8));",
        );
        return;
    }
    if let Some(unprefixed) = raw_payload_jvm.strip_prefix("Az") {
        if has_wrapper_class(unprefixed, ir) {
            builder.line(&format!("{} __nv = __ret.toNullable();", raw_payload_jvm));
            builder.line("if (__nv == null) return java.util.Optional.empty();");
            builder.line(&format!(
                "return java.util.Optional.of(new {}(__nv.getPointer()));",
                unprefixed
            ));
            return;
        }
    }
    builder.line("return java.util.Optional.ofNullable(__ret.toNullable());");
}

/// Emit the body of a wrapper method whose return is the bare Ok
/// payload of a `Result<T, E>` (throws `RuntimeException` on Err — the
/// FFI struct's `unwrap()` does that lift). Same three cases as
/// [`emit_option_return_body`].
fn emit_result_return_body(builder: &mut CodeBuilder, raw_payload_jvm: &str, ir: &CodegenIR) {
    if is_az_string_jvm(raw_payload_jvm, ir) {
        builder.line(&format!("{} __u = __ret.unwrap();", raw_payload_jvm));
        builder.line("Pointer __sp = __u.getPointer();");
        builder.line("Pointer __vecPtr = __sp.getPointer(0);");
        builder.line("long __vecLen = __sp.getLong(8);");
        builder.line("if (__vecPtr == null || __vecLen <= 0) return \"\";");
        builder.line("byte[] __bytes = __vecPtr.getByteArray(0, (int) __vecLen);");
        builder.line(
            "return new java.lang.String(__bytes, java.nio.charset.StandardCharsets.UTF_8);",
        );
        return;
    }
    if let Some(unprefixed) = raw_payload_jvm.strip_prefix("Az") {
        if has_wrapper_class(unprefixed, ir) {
            builder.line(&format!("{} __u = __ret.unwrap();", raw_payload_jvm));
            builder.line(&format!("return new {}(__u.getPointer());", unprefixed));
            return;
        }
    }
    builder.line("return __ret.unwrap();");
}

// ============================================================================
// Tagged-union helper class
// ============================================================================

fn emit_union_helper(builder: &mut CodeBuilder, e: &EnumDef) {
    let class_name = wrapper_class_name(&e.name);
    let ffi_name = ffi_type_name(&e.name);

    if !e.doc.is_empty() {
        builder.line("/**");
        for d in &e.doc {
            builder.line(&format!(" * {}", javadoc_escape(d)));
        }
        builder.line(" */");
    }

    builder.line(&format!("public final class {}Helpers {{", class_name));
    builder.indent();
    builder.line(&format!("private {}Helpers() {{}}", class_name));
    builder.blank();

    for v in &e.variants {
        match &v.kind {
            EnumVariantKind::Unit => {
                let mname = idiomatic_method_name(&v.name);
                let variant_ident = sanitize_identifier(&v.name);
                builder.line(&format!(
                    "/** Construct the {}.{} variant. */",
                    e.name, v.name
                ));
                builder.line(&format!(
                    "public static {} {}() {{",
                    ffi_name, mname
                ));
                builder.indent();
                builder.line(&format!("{} u = new {}();", ffi_name, ffi_name));
                builder.line(&format!(
                    "u.{}.tag = (byte) {}_Tag.{}.value;",
                    variant_ident, ffi_name, variant_ident
                ));
                builder.line(&format!("u.setType(\"{}\");", v.name));
                builder.line("return u;");
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
}

// ============================================================================
// Helpers
// ============================================================================

fn wrapper_class_name(raw: &str) -> String {
    sanitize_identifier(raw)
}

fn idiomatic_method_name(method_name: &str) -> String {
    if method_name == "new" {
        return "create".to_string();
    }
    let camel = if method_name.contains('_') {
        snake_to_lower_camel(method_name)
    } else {
        // Already lowerCamel (most common in api.json) or PascalCase;
        // ensure first letter is lowercase for Java methods.
        let mut chars = method_name.chars();
        match chars.next() {
            Some(c) => c.to_lowercase().collect::<String>() + chars.as_str(),
            None => String::new(),
        }
    };
    // `default`, `class`, `case`, etc. can't be method names. Java has
    // no verbatim-identifier syntax, so append `_`.
    // Also rename `close` because every wrapper implements AutoCloseable
    // with its own `close()` for resource cleanup — a user-API method
    // also named `close` would be a duplicate-definition error. The
    // SvgPath bug (an SVG path's "close path" segment vs the lifecycle
    // close) showed up first; the rule generalises.
    if super::is_java_reserved(&camel) {
        format!("{}_", camel)
    } else if camel == "close" {
        "closeInner".to_string()
    } else if matches!(camel.as_str(), "toString" | "hashCode" | "equals" | "getClass" | "clone" | "finalize") {
        // Methods declared on java.lang.Object have fixed signatures.
        // A user-API method called `toString` returning `AzString.ByValue`
        // cannot legally override `Object.toString()` (which returns
        // `java.lang.String`), so suffix it. Same family covers
        // hashCode / equals / clone / finalize / getClass.
        format!("{}_", camel)
    } else {
        camel
    }
}

fn javadoc_escape(s: &str) -> String {
    // Java's javadoc parser interprets `\u` / `\U` as Unicode escapes
    // (even inside comments — see JLS §3.3). Doc strings like
    // `C:\Users\username` contain `\U` which is parsed as the start of
    // an invalid Unicode escape sequence and rejected. Double the
    // backslashes so the literal text survives.
    s.replace('\\', "\\\\")
        .replace("*/", "*&#47;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('&', "&amp;")
}
