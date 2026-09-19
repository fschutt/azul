//! Idiomatic C# wrapper-class emission for the C# generator.
//!
//! For every IR struct that gets a wrapper class (the shared
//! `managed_lang_helpers::has_wrapper_class` rule: a `_delete` OR at least
//! one method in the C API) we emit a `public sealed class TypeName` that:
//!
//! - Holds the raw FFI struct (`AzTypeName`) by value in a private field
//! - Surfaces every non-trait method on `TypeName` as an idiomatic instance or static method that
//!   delegates to the underlying P/Invoke import
//! - When (and only when) the C API has `Az<TypeName>_delete`, implements `IDisposable` with a
//!   finalizer that calls it. Methods-only types (`CallbackInfo`, POD value types) own nothing
//!   and get neither.
//!
//! Ownership at method boundaries is keyed on `_delete` (see
//! [`is_owning_wrapper`]): owning types cross by-value call sites as
//! wrapper classes (consumed on transfer), value types as their raw
//! `Az*` structs. Callback arguments handed in by the engine are wrapped
//! as *borrowed* (`__Borrow`) so their methods work but nothing frees the
//! engine's bytes.
//!
//! Callback registration is typed end to end: `Button.OnClick<T>(T, HostInvoker.ButtonOnClickCallbackWithData<T>)`,
//! `WindowCreateOptions.Create<T>(HostInvoker.LayoutCallbackWithData<T>)`, `App.Create<T>(T, AppConfig)`
//! are emitted for every method matching the shared IR detectors
//! (`smart_callback_setter_info`, `layout_callback_factory_info`,
//! `app_factory_info`) whenever `managed::typed_delegate_info` can plumb
//! the callback kind.
//!
//! Tagged-union enums get a separate, very minimal "discriminator hierarchy"
//! emitted by [`generate_union_hierarchies`]; for now this is intentionally
//! narrow (we expose the tag enum and a static factory per unit variant).
//! Full pattern-matching support can be expanded later without breaking
//! the surface area.

use anyhow::Result;

use super::{
    super::{
        config::CodegenConfig,
        generator::CodeBuilder,
        ir::{
            ArgRefKind, CodegenIR, EnumDef, EnumVariantKind, FieldRefKind, FunctionArg,
            FunctionDef, FunctionKind, MonomorphizedKind, StructDef, TypeCategory,
        },
        managed_host_invoker::{
            app_factory_info, layout_callback_factory_info, managed_c_symbol,
            smart_callback_setter_info,
        },
        managed_lang_helpers::{has_delete_function, has_wrapper_class},
    },
    ffi_type_name, managed::typed_delegate_info_for_kind, map_type_to_csharp,
    sanitize_identifier, snake_to_pascal, types::ref_kind_field_type,
};

/// Phase I.5.2 (C#): how the wrapper method should idiomise an
/// `Option<T>` / `Result<T, E>` return. Mirrors the Java + Kotlin
/// `ReturnIdiom` enum (lang_java/wrappers.rs).
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

/// A wrapper class that OWNS native resources: it exists (shared
/// `has_wrapper_class`) and the C API has `Az<T>_delete`. This is the
/// predicate for ownership transfer at method boundaries — owning types
/// are passed/returned as wrapper classes (consumed on by-value
/// transfer), while methods-only value types (`ColorU`, `NodeId`, …)
/// cross as their raw `Az*` structs, exactly like in C.
fn is_owning_wrapper(type_name: &str, ir: &CodegenIR) -> bool {
    has_wrapper_class(type_name, ir) && has_delete_function(type_name, ir)
}

/// Map a payload's raw C# field type to the user-visible display type.
/// AzString → `string`; AzX with wrapper → `X`; else passthrough.
fn payload_display_cs(raw: &str, ir: &CodegenIR) -> String {
    if let Some(unprefixed) = raw.strip_prefix("Az") {
        if let Some(s) = ir.find_struct(unprefixed) {
            if matches!(s.category, TypeCategory::String) {
                return "string".to_string();
            }
        }
        if is_owning_wrapper(unprefixed, ir) {
            return unprefixed.to_string();
        }
    }
    raw.to_string()
}

fn is_az_string_cs(raw: &str, ir: &CodegenIR) -> bool {
    let Some(unprefixed) = raw.strip_prefix("Az") else {
        return false;
    };
    ir.find_struct(unprefixed)
        .map(|s| matches!(s.category, TypeCategory::String))
        .unwrap_or(false)
}

/// Build a `NativeMethods.Az<OptionT>_delete(...)` invocation that
/// frees the C-ABI-allocated Option/Result struct. C# requires a
/// heap-copy pointer (no `__ret.Pointer` accessor on the bare value-
/// type struct), so we marshal-out + free. None when no _delete
/// export exists in the IR.
fn format_option_delete_block_cs(option_type_name: &str, ir: &CodegenIR) -> Option<String> {
    use super::super::ir::FunctionKind;
    let has_delete = ir
        .functions
        .iter()
        .any(|f| f.class_name == option_type_name && matches!(f.kind, FunctionKind::Delete));
    if !has_delete {
        return None;
    }
    let ffi_name = ffi_type_name(option_type_name);
    // Returns a `{ ... }`-scoped block (single line) so multiple
    // emit_delete calls in the same method body don't collide on
    // local-variable names. The block marshals __ret to a heap-
    // copy IntPtr, calls _delete, frees the heap copy.
    Some(format!(
        "{{ var __del_ptr = \
         System.Runtime.InteropServices.Marshal.AllocHGlobal(System.Runtime.InteropServices.\
         Marshal.SizeOf<{ffi}>()); System.Runtime.InteropServices.Marshal.StructureToPtr(__ret, \
         __del_ptr, false); NativeMethods.{ffi}_delete(__del_ptr); \
         System.Runtime.InteropServices.Marshal.FreeHGlobal(__del_ptr); }}",
        ffi = ffi_name,
    ))
}

fn format_clone_call_cs(payload_type_name: &str, ir: &CodegenIR) -> Option<String> {
    use super::super::ir::FunctionKind;
    let has_clone = ir
        .functions
        .iter()
        .any(|f| f.class_name == payload_type_name && matches!(f.kind, FunctionKind::DeepCopy));
    if !has_clone {
        return None;
    }
    let ffi_name = ffi_type_name(payload_type_name);
    // The clone fn takes an IntPtr; the caller marshals the payload
    // first. We return the bare method name.
    Some(format!("NativeMethods.{}_clone", ffi_name))
}

/// Emit the body of an `Option<T>` return for C#. `__ret` (the AzOption
/// FFI struct value) has already been declared.
fn emit_cs_option_body(
    builder: &mut CodeBuilder,
    raw_payload_cs: &str,
    option_type_name: &str,
    ir: &CodegenIR,
) {
    let delete_block = format_option_delete_block_cs(option_type_name, ir);
    let emit_delete = |b: &mut CodeBuilder| {
        if let Some(block) = delete_block.as_ref() {
            b.line(block);
        }
    };
    if is_az_string_cs(raw_payload_cs, ir) {
        builder.line("var __nv = __ret.AsNullable();");
        builder.line("if (__nv == null) {");
        builder.indent();
        emit_delete(builder);
        builder.line("return null;");
        builder.dedent();
        builder.line("}");
        builder.line("var __azs = __nv.Value;");
        builder.line("var __vp = __azs.vec.ptr;");
        builder.line("var __vl = (long)__azs.vec.len.ToUInt64();");
        builder.line("string __out;");
        builder.line("if (__vp == System.IntPtr.Zero || __vl <= 0) {");
        builder.indent();
        builder.line("__out = \"\";");
        builder.dedent();
        builder.line("} else {");
        builder.indent();
        builder.line("var __bytes = new byte[__vl];");
        builder.line("System.Runtime.InteropServices.Marshal.Copy(__vp, __bytes, 0, (int)__vl);");
        builder.line("__out = System.Text.Encoding.UTF8.GetString(__bytes);");
        builder.dedent();
        builder.line("}");
        emit_delete(builder);
        builder.line("return __out;");
        return;
    }
    if let Some(unprefixed) = raw_payload_cs.strip_prefix("Az") {
        if is_owning_wrapper(unprefixed, ir) {
            let clone_call = format_clone_call_cs(unprefixed, ir);
            builder.line("var __nv = __ret.AsNullable();");
            builder.line("if (__nv == null) {");
            builder.indent();
            emit_delete(builder);
            builder.line("return null;");
            builder.dedent();
            builder.line("}");
            if let Some(ref clone) = clone_call {
                // Allocate a temp pointer for the payload, clone via
                // C ABI, wrap the clone, then drop the Option.
                builder.line(&format!(
                    "var __nv_ptr = \
                     System.Runtime.InteropServices.Marshal.AllocHGlobal(System.Runtime.\
                     InteropServices.Marshal.SizeOf<{}>());",
                    raw_payload_cs
                ));
                builder.line(&"System.Runtime.InteropServices.Marshal.StructureToPtr(__nv.Value, __nv_ptr, \
                     false);".to_string());
                builder.line(&format!("var __cloned = {}(__nv_ptr);", clone));
                builder.line("System.Runtime.InteropServices.Marshal.FreeHGlobal(__nv_ptr);");
                emit_delete(builder);
                builder.line(&format!("return new {}(__cloned);", unprefixed));
            } else {
                builder.line(&format!("return new {}(__nv.Value);", unprefixed));
            }
            return;
        }
    }
    builder.line("var __opt = __ret.AsNullable();");
    emit_delete(builder);
    builder.line("return __opt;");
}

fn emit_cs_result_body(
    builder: &mut CodeBuilder,
    raw_payload_cs: &str,
    result_type_name: &str,
    ir: &CodegenIR,
) {
    let delete_block = format_option_delete_block_cs(result_type_name, ir);
    let emit_delete = |b: &mut CodeBuilder| {
        if let Some(block) = delete_block.as_ref() {
            b.line(block);
        }
    };
    if is_az_string_cs(raw_payload_cs, ir) {
        builder.line("var __azs = __ret.Unwrap();");
        builder.line("var __vp = __azs.vec.ptr;");
        builder.line("var __vl = (long)__azs.vec.len.ToUInt64();");
        builder.line("string __out;");
        builder.line("if (__vp == System.IntPtr.Zero || __vl <= 0) {");
        builder.indent();
        builder.line("__out = \"\";");
        builder.dedent();
        builder.line("} else {");
        builder.indent();
        builder.line("var __bytes = new byte[__vl];");
        builder.line("System.Runtime.InteropServices.Marshal.Copy(__vp, __bytes, 0, (int)__vl);");
        builder.line("__out = System.Text.Encoding.UTF8.GetString(__bytes);");
        builder.dedent();
        builder.line("}");
        emit_delete(builder);
        builder.line("return __out;");
        return;
    }
    if let Some(unprefixed) = raw_payload_cs.strip_prefix("Az") {
        if is_owning_wrapper(unprefixed, ir) {
            let clone_call = format_clone_call_cs(unprefixed, ir);
            builder.line("var __u = __ret.Unwrap();");
            if let Some(ref clone) = clone_call {
                builder.line(&format!(
                    "var __u_ptr = \
                     System.Runtime.InteropServices.Marshal.AllocHGlobal(System.Runtime.\
                     InteropServices.Marshal.SizeOf<{}>());",
                    raw_payload_cs
                ));
                builder.line(
                    "System.Runtime.InteropServices.Marshal.StructureToPtr(__u, __u_ptr, false);",
                );
                builder.line(&format!("var __cloned = {}(__u_ptr);", clone));
                builder.line("System.Runtime.InteropServices.Marshal.FreeHGlobal(__u_ptr);");
                emit_delete(builder);
                builder.line(&format!("return new {}(__cloned);", unprefixed));
            } else {
                builder.line(&format!("return new {}(__u);", unprefixed));
            }
            return;
        }
    }
    builder.line("var __u = __ret.Unwrap();");
    emit_delete(builder);
    builder.line("return __u;");
}

// ============================================================================
// Public entry points (called from mod.rs)
// ============================================================================

pub fn generate_wrappers(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.line("// --------------------------------------------------------------------------");
    builder.line("// Idiomatic IDisposable wrapper classes.");
    builder.line("// --------------------------------------------------------------------------");
    builder.blank();

    // Finalizer-thread delete affinity guard. .NET finalizers run on a
    // dedicated GC finalizer thread; Az*_delete has no cross-thread
    // teardown guarantee while the native event loop is pumping on the
    // main thread (unlike Ruby/LuaJIT, whose GC finalizers run on the VM
    // thread, .NET genuinely crosses threads here). `App.Run` flips this
    // flag; `Dispose(false)` (the finalizer path) checks it and downgrades
    // a would-be cross-thread native delete to a leak-with-warning.
    // Proper deferred-delete draining on the main thread needs dll-side
    // cooperation (a lock-free delete queue drained by the event loop) —
    // documented follow-up.
    builder.line(
        "/// <summary>Internal: tracks whether the native event loop (App.Run) is active, to keep \
         GC-finalizer-thread deletes from racing it.</summary>",
    );
    builder.line("internal static class __AzAppLoopState");
    builder.line("{");
    builder.indent();
    builder.line("internal static volatile bool Running;");
    builder.dedent();
    builder.line("}");
    builder.blank();

    for s in &ir.structs {
        if !should_emit_wrapper(s, ir, config) {
            continue;
        }
        emit_wrapper_class(builder, s, ir);
    }

    Ok(())
}

pub fn generate_union_hierarchies(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    let mut emitted_header = false;

    for e in &ir.enums {
        if !should_emit_union_hierarchy(e, config) {
            continue;
        }

        if !emitted_header {
            builder.line(
                "// --------------------------------------------------------------------------",
            );
            builder.line("// Tagged-union convenience helpers (static factories per variant).");
            builder.line(
                "// --------------------------------------------------------------------------",
            );
            // A variant literally named `Main` yields a static factory
            // `Main()`, which the C# compiler reports as a non-entry-point
            // (CS0028) in every executable project. It is an ordinary
            // factory; scope the suppression to this section.
            builder.line("#pragma warning disable CS0028");
            builder.blank();
            emitted_header = true;
        }

        emit_union_helper(builder, e);
    }

    if emitted_header {
        builder.line("#pragma warning restore CS0028");
        builder.blank();
    }

    Ok(())
}

// ============================================================================
// Wrapper inclusion filter
// ============================================================================

/// Per-target inclusion on top of the shared wrapper-class rule. Every
/// other "does a wrapper class exist" question in this file and in
/// managed.rs goes through the same `has_wrapper_class`.
fn should_emit_wrapper(s: &StructDef, ir: &CodegenIR, config: &CodegenConfig) -> bool {
    config.should_include_type(&s.name) && has_wrapper_class(&s.name, ir)
}

fn should_emit_union_hierarchy(e: &EnumDef, config: &CodegenConfig) -> bool {
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

// ============================================================================
// Wrapper class emission
// ============================================================================

/// Phase I.1.5 (C#): Vec-shape detector. Same as Haskell H.3 / Java
/// I.1.2 / Kotlin I.1.3.
fn detect_vec_elem_type_cs(s: &StructDef) -> Option<String> {
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

fn emit_wrapper_class(builder: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR) {
    let has_delete = has_delete_function(&s.name, ir);
    let class_name = sanitize_class_name(&s.name);
    let ffi_name = ffi_type_name(&s.name);

    if !s.doc.is_empty() {
        for d in &s.doc {
            builder.line(&format!("/// <summary>{}</summary>", xml_escape(d)));
        }
    }

    // Phase I.1.5 (C#): when the struct is a Vec with a wrapper-class
    // element, declare `IEnumerable<T>` so `foreach (var x in vec)`
    // works.
    let vec_elem_type = detect_vec_elem_type_cs(s);
    let vec_elem_has_wrapper = |elem: &str| -> bool { is_owning_wrapper(elem, ir) };
    let extra_iface = match &vec_elem_type {
        Some(elem) if vec_elem_has_wrapper(elem) => {
            format!(
                ", System.Collections.Generic.IEnumerable<{}>",
                sanitize_class_name(elem)
            )
        }
        _ => String::new(),
    };

    builder.line(&format!(
        "{}",
        if has_delete { format!("public sealed class {} : IDisposable{}", class_name, extra_iface) } else { format!("public sealed class {}{}", class_name, if extra_iface.is_empty() { "".to_string() } else { format!(" : {}", &extra_iface[2..]) }) }
    ));
    builder.line("{");
    builder.indent();

    // Storage, disposal flag and (owning types only) the borrowed flag
    // set by `__Borrow` for engine-owned bytes.
    builder.line(&format!("private {} _inner;", ffi_name));
    builder.line("private bool _disposed;");
    if has_delete {
        builder.line("private bool _borrowed;");
    }
    builder.blank();

    // `Raw` accessor: returns the underlying FFI struct by value.
    // `public` so external assemblies (PowerShell scripts importing
    // the Azul module) can extract the raw struct when handing it to
    // a layout callback return.
    //
    // Disposed/consumed guard: after Dispose() the struct's heap
    // pointers are freed, and after __Consume() ownership moved to the
    // native side — handing `_inner` out in either state invites a
    // double-free (e.g. passing the same wrapper to two consuming call
    // sites). All codegen-emitted call sites read `Raw` BEFORE calling
    // `__Consume()`, so the guard only fires on genuine use-after-
    // dispose/-consume. Same exception shape as the method-entry guard
    // in `emit_wrapper_method`.
    builder.line(&"/// <summary>Returns the underlying FFI struct by value. Throws <see \
         cref=\"ObjectDisposedException\"/> if this wrapper was already disposed or consumed \
         (ownership transferred to the native side).</summary>".to_string());
    builder.line(&format!("public {} Raw", ffi_name));
    builder.line("{");
    builder.indent();
    builder.line("get");
    builder.line("{");
    builder.indent();
    builder.line(
        "if (_disposed) throw new ObjectDisposedException(nameof(_inner), \"wrapper already \
         disposed or consumed; its native data is no longer owned by this object\");",
    );
    builder.line("return _inner;");
    builder.dedent();
    builder.line("}");
    builder.dedent();
    builder.line("}");
    builder.blank();
    builder.line("/// <summary>Wrap an existing raw FFI struct (takes ownership).</summary>");
    builder.line(&format!(
        "internal {}({} inner) {{ _inner = inner; }}",
        class_name, ffi_name
    ));
    builder.blank();

    // Typed smart builders: `<Event><T>(T data, HostInvoker.<Kind>WithData<T> fn)`
    // for every `with_on_*(self, RefAny, <CallbackWrapperStruct>)` method
    // (shared detector), provided managed.rs emits the typed delegate for
    // that kind. There is deliberately NO `(object, Delegate)` overload:
    // since C# 10 any method group converts to `System.Delegate`, so such
    // an overload would silently accept a wrong signature and fail at
    // event time. With only the typed overload a mismatch is a compile
    // error. Kinds without a typed delegate keep the literal
    // `With<Event>(RefAny, <Kind>Callback)` method only.
    for func in ir.functions_for_class(&s.name) {
        let Some((smart_snake, wrapper_kind)) = smart_callback_setter_info(func) else {
            continue;
        };
        let Some(typed) = typed_delegate_info_for_kind(&wrapper_kind, ir) else {
            continue;
        };
        let smart_pascal = super::snake_to_pascal(&smart_snake);
        let with_pascal = super::snake_to_pascal(&func.method_name);
        builder.line("/// <summary>");
        builder.line(&format!(
            "/// Strongly-typed builder for {}: `data` is handed to `fn` as `T` on every event;",
            with_pascal
        ));
        builder.line("/// RefAny wrapping and host-invoker registration happen internally.");
        builder.line("/// </summary>");
        builder.line(&format!(
            "public {} {}<T>(T data, HostInvoker.{} fn) where T : class",
            class_name,
            smart_pascal,
            typed.delegate_name()
        ));
        builder.line("{");
        builder.indent();
        builder.line("var __data = HostInvoker.RefanyCreate(data);");
        builder.line(&format!(
            "var __cb = HostInvoker.Register{}<T>(fn);",
            wrapper_kind
        ));
        builder.line(&format!(
            "return {}(new RefAny(__data), new {}(__cb));",
            with_pascal, wrapper_kind
        ));
        builder.dedent();
        builder.line("}");
        builder.blank();
    }

    // AzString gets a `ToString()` override that decodes the wrapped
    // UTF-8 bytes into a managed `string`. AzString's C-side layout
    // is `{ vec: AzU8Vec }`, AzU8Vec is `{ ptr, len, cap, destructor }`,
    // so we read `_inner.vec.ptr` and `_inner.vec.len` and copy out.
    // (`len` is `UIntPtr`; route through ToUInt64 for portability.)
    if matches!(s.category, TypeCategory::String) {
        builder
            .line("/// <summary>Decode the wrapped UTF-8 bytes into a managed string.</summary>");
        builder.line("public override string ToString()");
        builder.line("{");
        builder.indent();
        builder.line("if (_disposed) return \"\";");
        builder.line("var ptr = _inner.vec.ptr;");
        builder.line("var len = (long)_inner.vec.len.ToUInt64();");
        builder.line("if (ptr == System.IntPtr.Zero || len <= 0) return \"\";");
        builder.line("var bytes = new byte[len];");
        builder.line("System.Runtime.InteropServices.Marshal.Copy(ptr, bytes, 0, (int)len);");
        builder.line("return System.Text.Encoding.UTF8.GetString(bytes);");
        builder.dedent();
        builder.line("}");
        builder.blank();
    }

    // Typed smart factory `<Class>.Create<T>(HostInvoker.<Kind>WithData<T> fn)`
    // for every struct matching the shared layout-callback-factory shape
    // (a `_default` factory plus a `create(<Kind>Type)` constructor,
    // today `WindowCreateOptions`). The registered callback struct is
    // spliced into the `_default` value at the IR-discovered field path:
    // C# struct-field assignment is a byte copy, so intermediate structs
    // along the path are copied out, patched and re-assigned upwards.
    // Untyped `Create(Delegate)` is intentionally not emitted (see the
    // smart-builder note above); the literal `Create(Az<Kind>Type)` from
    // api.json remains for raw function-pointer users.
    if let Some(info) = layout_callback_factory_info(s, ir) {
        if let Some(typed) = typed_delegate_info_for_kind(&info.callback_wrapper, ir) {
            let wrapper_class = info.class_name.clone();
            builder.line("/// <summary>");
            builder.line(&format!(
                "/// Strongly-typed factory: registers `fn` as the {} and returns a default",
                info.callback_wrapper
            ));
            builder.line(&format!(
                "/// {} carrying it. `T` is the model type registered with the application \
                 factory (`App.Create&lt;T&gt;`).",
                wrapper_class
            ));
            builder.line("/// </summary>");
            builder.line(&format!(
                "public static {} Create<T>(HostInvoker.{} fn) where T : class",
                wrapper_class,
                typed.delegate_name()
            ));
            builder.line("{");
            builder.indent();
            builder.line(&format!(
                "var __cb = HostInvoker.Register{}<T>(fn);",
                info.callback_wrapper
            ));
            builder.line(&format!(
                "var __wco = NativeMethods.{}();",
                info.default_c_name
            ));
            emit_cs_field_path_splice(builder, "__wco", "__cb", &info.field_path);
            builder.line(&format!("return new {}(__wco);", wrapper_class));
            builder.dedent();
            builder.line("}");
            builder.blank();
        }
    }

    // Emit methods for each non-trait function on this class.
    for func in ir.functions_for_class(&s.name) {
        if func.kind.is_trait_function() {
            // Skip Delete/PartialEq/Cmp/Hash/Debug — Dispose() and overrides cover these.
            continue;
        }
        emit_wrapper_method(builder, &class_name, func, ir);
    }

    // Phase I.2 (C#): override Equals(object) + GetHashCode() routed
    // through the codegen-emitted `Az<X>_partialEq` / `Az<X>_hash`
    // exports when TypeTraits says they're supported.
    emit_cs_equals_hashcode_if_supported(builder, s, &class_name, ir);

    // Phase I.3 (C#): override ToString() through Az<X>_toDbgString.
    emit_cs_to_string_if_supported(builder, s, ir);

    // Phase I.1.5 (C#): GetEnumerator() body for Vec wrappers.
    // Wrapper-element Vecs get IEnumerable<T>; primitive-element
    // Vecs get a bulk-copy `ToXxxArray()` sibling instead.
    if let Some(elem) = vec_elem_type.as_deref() {
        if vec_elem_has_wrapper(elem) {
            emit_cs_vec_enumerator(builder, s, elem, ir);
        } else {
            emit_cs_vec_primitive_array(builder, s, elem);
        }
    }

    // IDisposable boilerplate (only for owning types) + __Consume/__Borrow.
    emit_dispose_methods(builder, &class_name, &s.name, ir);

    // Typed application factory `Create<T>(T data, <Config> config)` for
    // the class matching the shared app-factory shape: a constructor
    // taking exactly (owned RefAny, owned struct-with-`_default`) plus a
    // window-taking method (`App.create(RefAny, AppConfig)` + `App.run`).
    // The model is wrapped into a host-handle RefAny and forwarded to the
    // literal constructor wrapper emitted above.
    if let Some(app) = app_factory_info(ir).filter(|a| a.class_name == s.name) {
        let config_class = sanitize_class_name(&app.config_type);
        let mut params = vec![String::new(); 2];
        params[app.data_arg_index] = "T data".to_string();
        params[app.config_arg_index] = format!("{} config", config_class);
        let mut args = vec![String::new(); 2];
        args[app.data_arg_index] = "new RefAny(HostInvoker.RefanyCreate(data))".to_string();
        args[app.config_arg_index] = "config".to_string();
        let ctor = ir
            .functions
            .iter()
            .find(|f| f.c_name == app.create_c_name)
            .map(|f| idiomatic_method_name(&f.method_name))
            .unwrap_or_else(|| "Create".to_string());
        builder.line("/// <summary>");
        builder.line(&format!(
            "/// Strongly-typed factory: wraps `data` into the RefAny that every callback registered"
        ));
        builder.line(&format!(
            "/// with the same model (e.g. through `{}.Create&lt;T&gt;`) receives as `T`.",
            app.window_methods
                .first()
                .map(|(_, _, w)| w.class_name.as_str())
                .unwrap_or("WindowCreateOptions")
        ));
        builder.line("/// </summary>");
        builder.line(&format!(
            "public static {} {}<T>({}) where T : class",
            class_name,
            ctor,
            params.join(", ")
        ));
        builder.line("{");
        builder.indent();
        builder.line(&format!("return {}({});", ctor, args.join(", ")));
        builder.dedent();
        builder.line("}");
        builder.blank();
    }

    builder.dedent();
    builder.line("}");
    builder.blank();
}

/// Splice `value` into `root.<path>` where `path` walks nested by-value
/// structs: copy each intermediate out, assign the leaf, re-assign the
/// intermediates back up (a C# struct field read is a copy). Path
/// length 1 collapses to a single assignment.
fn emit_cs_field_path_splice(builder: &mut CodeBuilder, root: &str, value: &str, path: &[String]) {
    let depth = path.len();
    let var_for = |i: usize| -> String {
        if i == 0 {
            root.to_string()
        } else {
            format!("__lvl{}", i - 1)
        }
    };
    for (i, seg) in path.iter().enumerate().take(depth.saturating_sub(1)) {
        builder.line(&format!("var __lvl{} = {}.{};", i, var_for(i), seg));
    }
    let Some(leaf) = path.last() else {
        return;
    };
    builder.line(&format!("{}.{} = {};", var_for(depth - 1), leaf, value));
    for i in (0..depth.saturating_sub(1)).rev() {
        builder.line(&format!("{}.{} = __lvl{};", var_for(i), path[i], i));
    }
}

/// Phase I.2 (C#): override Equals(object) + GetHashCode() to route
/// through the codegen-emitted `Az<X>_partialEq` / `Az<X>_hash` C
/// exports when TypeTraits flags them. Pure type-driven.
fn emit_cs_equals_hashcode_if_supported(
    builder: &mut CodeBuilder,
    s: &StructDef,
    class_name: &str,
    ir: &CodegenIR,
) {
    let eq_sym = format!("Az{}_partialEq", s.name);
    let has_eq = s.traits.is_partial_eq && ir.functions.iter().any(|f| f.c_name == eq_sym);
    let hash_sym = format!("Az{}_hash", s.name);
    let has_hash = s.traits.is_hash && ir.functions.iter().any(|f| f.c_name == hash_sym);

    if has_eq {
        builder.line(&format!(
            "/// <summary>Equality routed through {}.</summary>",
            eq_sym
        ));
        // Plain `object` (not `object?`): PowerShell's Add-Type embeds this
        // C# without a `#nullable` context, so the nullable-reference
        // annotation `object?` trips CS8632. Mirrors managed.rs's handle-store
        // signatures. `dotnet`/csc accept plain `object` in both contexts.
        builder.line("public override bool Equals(object other)");
        builder.line("{");
        builder.indent();
        builder.line(&format!("if (other is not {} o) return false;", class_name));
        builder.line("if (_disposed || o._disposed) return false;");
        // The native helpers take `const Az<X>*`. We marshal _inner to a
        // heap copy (matching the same pattern other instance methods
        // use), call the helper, then free.
        builder.line(&format!(
            "var sz = System.Runtime.InteropServices.Marshal.SizeOf<Az{}>();",
            s.name
        ));
        builder.line("var aPtr = System.Runtime.InteropServices.Marshal.AllocHGlobal(sz);");
        builder.line("var bPtr = System.Runtime.InteropServices.Marshal.AllocHGlobal(sz);");
        builder.line("try");
        builder.line("{");
        builder.indent();
        builder.line("System.Runtime.InteropServices.Marshal.StructureToPtr(_inner, aPtr, false);");
        builder
            .line("System.Runtime.InteropServices.Marshal.StructureToPtr(o._inner, bPtr, false);");
        builder.line(&format!("return NativeMethods.{}(aPtr, bPtr);", eq_sym));
        builder.dedent();
        builder.line("}");
        builder.line("finally");
        builder.line("{");
        builder.indent();
        builder.line("System.Runtime.InteropServices.Marshal.FreeHGlobal(aPtr);");
        builder.line("System.Runtime.InteropServices.Marshal.FreeHGlobal(bPtr);");
        builder.dedent();
        builder.line("}");
        builder.dedent();
        builder.line("}");
        builder.blank();
    }

    if has_hash {
        builder.line(&format!(
            "/// <summary>Hash routed through {}.</summary>",
            hash_sym
        ));
        builder.line("public override int GetHashCode()");
        builder.line("{");
        builder.indent();
        builder.line("if (_disposed) return 0;");
        builder.line(&format!(
            "var sz = System.Runtime.InteropServices.Marshal.SizeOf<Az{}>();",
            s.name
        ));
        builder.line("var p = System.Runtime.InteropServices.Marshal.AllocHGlobal(sz);");
        builder.line("try");
        builder.line("{");
        builder.indent();
        builder.line("System.Runtime.InteropServices.Marshal.StructureToPtr(_inner, p, false);");
        builder.line(&format!("var h = NativeMethods.{}(p);", hash_sym));
        builder.line("return (int)(h ^ (h >> 32));");
        builder.dedent();
        builder.line("}");
        builder.line("finally { System.Runtime.InteropServices.Marshal.FreeHGlobal(p); }");
        builder.dedent();
        builder.line("}");
        builder.blank();
    } else if has_eq {
        // Equals compares VALUES (the C `_partialEq`) and the type has no C
        // `_hash`: equal values must hash equal, so the only hash that keeps
        // the contract is a constant (the struct's own hash includes its
        // pointers, which differ between two equal values).
        builder.line("/// <summary>Constant: equal values must hash equal, and the type has no value hash.</summary>");
        builder.line("public override int GetHashCode() => 0;");
        builder.blank();
    }
}

/// Phase I.3 (C#): override ToString() through Az<X>_toDbgString.
fn emit_cs_to_string_if_supported(builder: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR) {
    if matches!(s.category, TypeCategory::String) {
        return;
    }
    let dbg_sym = format!("Az{}_toDbgString", s.name);
    let has_dbg = s.traits.is_debug && ir.functions.iter().any(|f| f.c_name == dbg_sym);
    if !has_dbg {
        return;
    }
    // Skip when the user-facing surface already defines `ToString()`
    // (e.g. `AzUrl_toString` / `AzJson_toString` map to `Url.ToString` /
    // `Json.ToString`). Avoid CS0111 duplicate-member errors.
    if ir
        .functions
        .iter()
        .any(|f| f.class_name == s.name && idiomatic_method_name(&f.method_name) == "ToString")
    {
        return;
    }
    builder.line(&format!(
        "/// <summary>String repr routed through {}.</summary>",
        dbg_sym
    ));
    builder.line("public override string ToString()");
    builder.line("{");
    builder.indent();
    builder.line("if (_disposed) return base.ToString() ?? \"\";");
    // Marshal _inner to AllocHGlobal'd pointer (same pattern as Equals).
    builder.line(&format!(
        "var sz = System.Runtime.InteropServices.Marshal.SizeOf<Az{}>();",
        s.name
    ));
    builder.line("var p = System.Runtime.InteropServices.Marshal.AllocHGlobal(sz);");
    builder.line("try");
    builder.line("{");
    builder.indent();
    builder.line("System.Runtime.InteropServices.Marshal.StructureToPtr(_inner, p, false);");
    builder.line(&format!("var s = NativeMethods.{}(p);", dbg_sym));
    // Decode AzString via marshal to pointer, read vec.ptr/.len, free.
    builder.line(
        "var sPtr = \
         System.Runtime.InteropServices.Marshal.AllocHGlobal(System.Runtime.InteropServices.\
         Marshal.SizeOf<AzString>());",
    );
    builder.line("try");
    builder.line("{");
    builder.indent();
    builder.line("System.Runtime.InteropServices.Marshal.StructureToPtr(s, sPtr, false);");
    builder.line("var vecPtr = System.Runtime.InteropServices.Marshal.ReadIntPtr(sPtr, 0);");
    builder.line(
        "var vecLen = (int)System.Runtime.InteropServices.Marshal.ReadInt64(sPtr, \
         System.IntPtr.Size);",
    );
    builder.line("if (vecPtr == System.IntPtr.Zero || vecLen <= 0) return \"\";");
    builder.line("var bytes = new byte[vecLen];");
    builder.line("System.Runtime.InteropServices.Marshal.Copy(vecPtr, bytes, 0, vecLen);");
    builder.line("var result = System.Text.Encoding.UTF8.GetString(bytes);");
    builder.line("NativeMethods.AzString_delete(sPtr);");
    builder.line("return result;");
    builder.dedent();
    builder.line("}");
    builder.line("finally { System.Runtime.InteropServices.Marshal.FreeHGlobal(sPtr); }");
    builder.dedent();
    builder.line("}");
    builder.line("finally { System.Runtime.InteropServices.Marshal.FreeHGlobal(p); }");
    builder.dedent();
    builder.line("}");
    builder.blank();
}

/// IEnumerable<T> GetEnumerator() body for a Vec wrapper.
///
/// Each yielded element is deep-cloned via the element type's
/// `_clone` C export so the wrapper owns its own heap allocations
/// and survives the Vec being disposed. If `_clone` isn't
/// available, fall back to a buffer-borrowed wrapper marked
/// consumed (its `Dispose()` is a no-op so no AzX_delete on
/// Vec-internal memory).
/// Primitive-element Vec sibling: bulk-copy via `Marshal.Copy` into
/// a C# native typed array (`byte[]`/`int[]`/`long[]`/...). One
/// memcpy — fully independent of the Vec's lifetime.
fn emit_cs_vec_primitive_array(builder: &mut CodeBuilder, _s: &StructDef, elem_rust: &str) {
    let (cs_ty, copy_arr_ty, method_name) = match elem_rust.trim() {
        "u8" | "i8" | "bool" => ("byte", "byte", "ToByteArray"),
        "u16" | "i16" => ("short", "short", "ToShortArray"),
        "u32" | "i32" => ("int", "int", "ToIntArray"),
        "u64" | "i64" | "usize" | "isize" => ("long", "long", "ToLongArray"),
        "f32" => ("float", "float", "ToFloatArray"),
        "f64" => ("double", "double", "ToDoubleArray"),
        _ => return,
    };
    builder.line(&format!(
        "/// <summary>Bulk-copy the Vec's `{}` elements into a {}[] (one Marshal.Copy, \
         GC-owned).</summary>",
        elem_rust, cs_ty
    ));
    builder.line(&format!("public {}[] {}()", cs_ty, method_name));
    builder.line("{");
    builder.indent();
    builder.line(&format!(
        "if (_disposed) return System.Array.Empty<{}>();",
        cs_ty
    ));
    builder.line("var __buf = _inner.ptr;");
    builder.line("var __n = (long)_inner.len;");
    builder.line(&format!(
        "if (__buf == System.IntPtr.Zero || __n <= 0) return System.Array.Empty<{}>();",
        cs_ty
    ));
    builder.line(&format!("var __out = new {}[(int)__n];", cs_ty));
    builder.line("System.Runtime.InteropServices.Marshal.Copy(__buf, __out, 0, (int)__n);");
    builder.line("return __out;");
    builder.dedent();
    builder.line("}");
    builder.blank();
    let _ = copy_arr_ty;
}

fn emit_cs_vec_enumerator(
    builder: &mut CodeBuilder,
    _s: &StructDef,
    elem_type: &str,
    ir: &CodegenIR,
) {
    let elem_ffi = format!("Az{}", elem_type);
    let elem_wrapper = sanitize_class_name(elem_type);
    let clone_call = format_clone_call_cs(elem_type, ir);

    builder.line(&format!(
        "/// <summary>Iterate the Vec yielding {} elements.</summary>",
        elem_wrapper
    ));
    if clone_call.is_some() {
        builder.line(
            "/// <remarks>Each element is deep-cloned via _clone; safe past Vec dispose.</remarks>",
        );
    } else {
        builder.line(
            "/// <remarks>Buffer-borrowed iteration (no _clone available); don't keep yielded \
             wrappers past the Vec's lifetime.</remarks>",
        );
    }
    builder.line(&format!(
        "public System.Collections.Generic.IEnumerator<{}> GetEnumerator()",
        elem_wrapper
    ));
    builder.line("{");
    builder.indent();
    builder.line("if (_disposed) yield break;");
    builder.line("var __buf = _inner.ptr;");
    builder.line("var __n = (long)_inner.len;");
    builder.line(&format!(
        "var __sz = System.Runtime.InteropServices.Marshal.SizeOf<{}>();",
        elem_ffi
    ));
    builder.line("for (long __i = 0; __i < __n; __i++)");
    builder.line("{");
    builder.indent();
    builder.line("var __ep = System.IntPtr.Add(__buf, (int)(__i * __sz));");
    if let Some(ref clone) = clone_call {
        builder.line(&format!("var __cloned = {}(__ep);", clone));
        builder.line(&format!("yield return new {}(__cloned);", elem_wrapper));
    } else {
        builder.line(&format!(
            "var __ev = System.Runtime.InteropServices.Marshal.PtrToStructure<{}>(__ep);",
            elem_ffi
        ));
        builder.line(&format!(
            "yield return {}.__Borrow(__ev);",
            elem_wrapper
        ));
    }
    builder.dedent();
    builder.line("}");
    builder.dedent();
    builder.line("}");
    builder.line(
        "System.Collections.IEnumerator System.Collections.IEnumerable.GetEnumerator() => \
         GetEnumerator();",
    );
    builder.blank();
}

/// Lifetime members of a wrapper class.
///
/// * Owning types (`Az<X>_delete` exists): `Dispose()`, `Dispose(bool)`
///   with the finalizer-thread guard, `~X()`, plus a `_borrowed` flag
///   that turns both into no-ops for engine-owned bytes.
/// * Methods-only types: nothing to free, so no `IDisposable`, no
///   finalizer (a finalizer on a POD value type would be pure overhead
///   and would have to call a `_delete` that does not exist).
/// * Every type: `__Consume()` (ownership moved to the C ABI by value)
///   and `__Borrow(raw)` (wrap engine-owned bytes for the duration of a
///   callback: methods work, nothing is ever freed).
fn emit_dispose_methods(
    builder: &mut CodeBuilder,
    class_name: &str,
    raw_type_name: &str,
    ir: &CodegenIR,
) {
    let has_delete = has_delete_function(raw_type_name, ir);
    if has_delete {
        builder.line("/// <summary>Frees the underlying native resources.</summary>");
        builder.line("public void Dispose()");
        builder.line("{");
        builder.indent();
        builder.line("Dispose(true);");
        builder.line("GC.SuppressFinalize(this);");
        builder.dedent();
        builder.line("}");
        builder.blank();

        builder.line("private void Dispose(bool disposing)");
        builder.line("{");
        builder.indent();
        builder.line("if (_disposed) return;");
        builder.line("// Borrowed from the engine (callback argument): never free.");
        builder.line("if (_borrowed) { _disposed = true; return; }");
        builder.line("// Finalizer-thread affinity guard: `disposing == false` means we're");
        builder.line("// on the GC finalizer thread. While App.Run is pumping the native");
        builder.line("// event loop on the main thread, Az*_delete from another thread has");
        builder.line("// no teardown guarantee — downgrade the potential cross-thread crash");
        builder.line("// to a leak-with-warning. Dispose() explicitly (or keep the object");
        builder.line("// referenced) to avoid the leak. Deterministic deferred deletion");
        builder.line("// needs a dll-side delete queue drained by the event loop (follow-up).");
        builder.line("if (!disposing && __AzAppLoopState.Running)");
        builder.line("{");
        builder.indent();
        builder.line("_disposed = true;");
        builder.line(&format!(
            "System.Console.Error.WriteLine(\"[azul] warning: {} finalized on the GC thread while \
             the App event loop is running; skipping the native delete (memory leaked). Call \
             Dispose() before App.Run or keep the object alive.\");",
            class_name
        ));
        builder.line("return;");
        builder.dedent();
        builder.line("}");
        builder.line("// `disposing` is false when called from the finalizer; native");
        builder.line("// cleanup is still safe because the FFI struct is value-typed.");
        // Use Marshal.AllocHGlobal/StructureToPtr instead of `fixed` so the
        // emit is compatible with PowerShell's Add-Type (PS 7's Roslyn
        // wrapper has no /unsafe option). Slight overhead — one extra alloc
        // — but the call is at Dispose time only.
        builder.line(&format!(
            "var __p = System.Runtime.InteropServices.Marshal.AllocHGlobal(System.Runtime.\
             InteropServices.Marshal.SizeOf<{}>());",
            ffi_type_name(raw_type_name)
        ));
        builder.line("System.Runtime.InteropServices.Marshal.StructureToPtr(_inner, __p, false);");
        builder.line(&format!("NativeMethods.Az{}_delete(__p);", raw_type_name));
        builder.line("System.Runtime.InteropServices.Marshal.FreeHGlobal(__p);");
        builder.line("_disposed = true;");
        builder.dedent();
        builder.line("}");
        builder.blank();

        builder.line(&format!("~{}() {{ Dispose(false); }}", class_name));
        builder.blank();
    }

    // Mark this wrapper as consumed without calling Az<X>_delete.
    // Used by codegen-emitted call sites where the C ABI takes
    // ownership of the underlying bytes by-value (DeepCopy `with_*`
    // methods, owned-by-value wrapper args, typed-delegate return
    // splice). Sets `_disposed = true` and suppresses the finalizer
    // so the deferred ~ClassName() doesn't double-drop.
    builder.line(
        "/// <summary>Internal: mark consumed (used by codegen-emitted bridges that transfer \
         ownership by-value to the C ABI).</summary>",
    );
    builder.line("internal void __Consume()");
    builder.line("{");
    builder.indent();
    builder.line("_disposed = true;");
    builder.line("System.GC.SuppressFinalize(this);");
    builder.dedent();
    builder.line("}");
    builder.blank();

    // Borrow: wrap bytes the engine still owns (callback arguments,
    // Vec elements without `_clone`). Unlike __Consume the methods stay
    // usable; unlike the plain constructor nothing is ever freed.
    builder.line(
        "/// <summary>Internal: wrap engine-owned bytes without taking ownership (callback \
         arguments). Methods work; Dispose() and the finalizer never free them.</summary>",
    );
    builder.line(&format!(
        "internal static {} __Borrow({} inner)",
        class_name,
        ffi_type_name(raw_type_name)
    ));
    builder.line("{");
    builder.indent();
    builder.line(&format!("var __w = new {}(inner);", class_name));
    if has_delete {
        builder.line("__w._borrowed = true;");
        builder.line("System.GC.SuppressFinalize(__w);");
    }
    builder.line("return __w;");
    builder.dedent();
    builder.line("}");
    builder.blank();
}

// ============================================================================
// Method emission
// ============================================================================

fn emit_wrapper_method(
    builder: &mut CodeBuilder,
    class_name: &str,
    func: &FunctionDef,
    ir: &CodegenIR,
) {
    let method_name = idiomatic_method_name(&func.method_name);
    let ffi_class_name = ffi_type_name(&func.class_name);

    // Skip auto-generated default constructors with no body — there's
    // nothing meaningful to surface beyond `new T()`.
    let _ = func.is_const;

    let return_cs = func
        .return_type
        .as_ref()
        .map(|r| map_type_to_csharp(r, ir))
        .unwrap_or_else(|| "void".to_string());

    // The first parameter of an instance/clone/deepcopy method is the
    // implicit self pointer. `func.args[0]` is the self regardless of
    // its declared name in api.json (which can be the lowercased class,
    // `self`, or — in the trait-impl case — synonyms like `instance`).
    // Skip args[0] for takes_self methods to avoid double-counting it as
    // a user argument.
    let takes_self = matches!(
        func.kind,
        FunctionKind::Method | FunctionKind::MethodMut | FunctionKind::DeepCopy
    );
    let user_args: Vec<_> = if takes_self {
        func.args.iter().skip(1).collect()
    } else {
        let class_lower = func.class_name.to_lowercase();
        func.args
            .iter()
            .filter(|a| a.name != class_lower && a.name != "self")
            .collect()
    };

    // Auto-conversion rules (mirrors Java/Kotlin; pure type-driven, no
    // method-name allowlist):
    // 1. Owned `String` arg → param takes `string`; emit UTF-8 → AzString_fromUtf8 conversion at
    //    the start of the body.
    // 2. Owned wrapper-class arg → param takes the wrapper class (e.g. `Dom child` rather than
    //    `AzDom child`); the call site reaches into `child.Raw` (every emitted wrapper class
    //    exposes `Raw => _inner`).
    let is_az_string_owned_arg = |a: &&FunctionArg| -> bool {
        a.type_name.trim() == "String" && matches!(a.ref_kind, ArgRefKind::Owned)
    };
    // Owned arg of an OWNING wrapper type: the C ABI takes the bytes by
    // value, so the wrapper is consumed after the call. (`String` has an
    // owning wrapper too but is handled by rule 1 above.)
    let is_wrapper_class_owned_arg = |a: &&FunctionArg| -> bool {
        matches!(a.ref_kind, ArgRefKind::Owned)
            && a.type_name.trim() != "String"
            && is_owning_wrapper(a.type_name.trim(), ir)
    };
    // Raw callback-typedef arg (`AzLayoutCallbackType cb`): P/Invoke
    // marshals the delegate to a function pointer but does not root it,
    // and libazul keeps the pointer — pin the delegate for the process
    // lifetime before the call.
    let is_raw_callback_arg = |a: &&FunctionArg| -> bool {
        let tn = a.type_name.trim();
        ir.callback_typedefs.iter().any(|c| c.name == tn)
    };

    // Build argument signature.
    let arg_sig: Vec<String> = user_args
        .iter()
        .map(|a| {
            let cs_type = if is_az_string_owned_arg(a) {
                "string".to_string()
            } else if is_wrapper_class_owned_arg(a) {
                // C# wrapper class name strips the `Az` prefix.
                a.type_name.trim().to_string()
            } else {
                match a.ref_kind {
                    ArgRefKind::Owned => map_type_to_csharp(&a.type_name, ir),
                    ArgRefKind::Ref | ArgRefKind::RefMut | ArgRefKind::Ptr | ArgRefKind::PtrMut => {
                        "IntPtr".to_string()
                    }
                }
            };
            format!("{} {}", cs_type, sanitize_identifier(&a.name))
        })
        .collect();

    // Determine how the C ABI receives the implicit self:
    // - `Owned` (by-value) → pass `_inner` directly, no Marshal alloc.
    // - `Ref/RefMut/Ptr/PtrMut` → IntPtr to a heap-copy via `__self`.
    let self_by_value = takes_self
        && func
            .args
            .first()
            .map(|a| matches!(a.ref_kind, ArgRefKind::Owned))
            .unwrap_or(false);
    let mut call_args: Vec<String> = Vec::new();
    // Names to mark consumed (no-op their finalizer's Az<X>_delete)
    // after the C-ABI call: any wrapper passed by-value, plus the
    // implicit `this` when self_by_value.
    let mut consume_after_call: Vec<String> = Vec::new();
    if takes_self {
        if self_by_value {
            call_args.push("_inner".to_string());
            // DeepCopy / consuming-self method.
            consume_after_call.push("this".to_string());
        } else {
            call_args.push("(IntPtr)__self".to_string());
        }
    }
    // Callback args: no auto-substitution at the wrapper-method
    // boundary. The wrapper's parameter type matches the C ABI
    // (`AzCallback` struct or `AzCallbackType` typedef) and is passed
    // through unchanged; the typed smart builders/factories emitted in
    // `emit_wrapper_class` are the idiomatic entry points. A raw
    // typedef delegate is pinned (see `is_raw_callback_arg`).
    //
    // Auto-string-conversion: see rule predicate above. Owned `String`
    // args get a UTF-8-bytes → AzString_fromUtf8 conversion emitted at
    // the start of the method body; the call site uses the converted
    // local name instead of the raw parameter.
    let mut pre_call_lines: Vec<String> = Vec::new();
    for a in &user_args {
        let raw_name = sanitize_identifier(&a.name);
        if is_raw_callback_arg(a) {
            pre_call_lines.push(format!("HostInvoker.__Pin({});", raw_name));
            call_args.push(raw_name);
        } else if is_az_string_owned_arg(a) {
            // C# keyword-escapes with `@` prefix; strip it so local var
            // names like `__@class_bytes` (invalid) become
            // `__class_bytes` (valid).
            let stem = raw_name.trim_start_matches('@');
            let az_name = format!("__{}_az", stem);
            let bytes_name = format!("__{}_bytes", stem);
            let ptr_name = format!("__{}_ptr", stem);
            pre_call_lines.push(format!(
                "var {bytes} = System.Text.Encoding.UTF8.GetBytes({raw});",
                bytes = bytes_name,
                raw = raw_name,
            ));
            pre_call_lines.push(format!(
                "var {ptr} = System.Runtime.InteropServices.Marshal.AllocHGlobal({bytes}.Length);",
                ptr = ptr_name,
                bytes = bytes_name,
            ));
            pre_call_lines.push(format!(
                "System.Runtime.InteropServices.Marshal.Copy({bytes}, 0, {ptr}, {bytes}.Length);",
                bytes = bytes_name,
                ptr = ptr_name,
            ));
            pre_call_lines.push(format!(
                "var {az} = NativeMethods.AzString_fromUtf8({ptr}, (UIntPtr){bytes}.Length);",
                az = az_name,
                ptr = ptr_name,
                bytes = bytes_name,
            ));
            // Free the temp buffer once AzString_fromUtf8 has owned the
            // bytes (the AzString takes a copy internally).
            pre_call_lines.push(format!(
                "System.Runtime.InteropServices.Marshal.FreeHGlobal({ptr});",
                ptr = ptr_name,
            ));
            call_args.push(az_name);
        } else if is_wrapper_class_owned_arg(a) {
            // Every wrapper class exposes `public Az<T> Raw => _inner;`
            // — reach in directly. No struct-to-pointer marshal needed
            // because C# struct-field-assignment is a byte copy.
            call_args.push(format!("{}.Raw", raw_name));
            // C ABI takes by-value; the caller's wrapper is now
            // ownership-transferred. Mark consumed.
            consume_after_call.push(raw_name.clone());
        } else {
            call_args.push(raw_name);
        }
    }

    let is_static = matches!(
        func.kind,
        FunctionKind::Constructor | FunctionKind::StaticMethod | FunctionKind::Default
    );

    // Emit doc comment.
    if !func.doc.is_empty() {
        for d in &func.doc {
            builder.line(&format!("/// <summary>{}</summary>", xml_escape(d)));
        }
    }

    // Constructors and static factories return a wrapper around the
    // returned FFI struct; instance methods simply delegate.
    //
    // Methods inherited from `System.Object` (ToString, GetHashCode,
    // Equals, GetType) need `new` (or `override`) when our signature
    // SHADOWS the inherited one. Only fires for exact-signature
    // collisions — `GetType()` shadows, but `GetType(arg)` doesn't.
    // The common collider in practice is `ToString()` returning
    // `Az*` instead of `string`.
    let arg_count = arg_sig.len();
    let needs_new = !is_static
        && match method_name.as_str() {
            "ToString" => arg_count == 0,
            "GetHashCode" => arg_count == 0,
            "GetType" => arg_count == 0,
            "Equals" => arg_count == 1,
            "MemberwiseClone" => arg_count == 0,
            _ => false,
        };
    let modifiers = if is_static {
        "public static".to_string()
    } else if needs_new {
        "public new".to_string()
    } else {
        "public".to_string()
    };

    // Decide whether the return type should be wrapped. If the FFI
    // function returns the same struct as the wrapping class, wrap it.
    let returns_self = func
        .return_type
        .as_deref()
        .map(|r| r.trim() == func.class_name)
        .unwrap_or(false);

    let idiom = classify_return(func, ir);

    // Auto-wrap non-self OWNING wrapper-class returns (the caller now
    // owns the bytes and must be able to Dispose them); value types are
    // returned as raw structs.
    let returns_wrapper_other: Option<String> = if returns_self {
        None
    } else if matches!(idiom, ReturnIdiom::Plain) {
        func.return_type
            .as_deref()
            .map(|r| r.trim())
            .filter(|r| is_owning_wrapper(r, ir))
            .map(|r| r.to_string())
    } else {
        None
    };

    // IR-side name of the Option/Result return type, used to look up
    // the matching _delete export so we can free the wrapper's outer
    // heap allocations after payload extraction.
    let option_or_result_ty = func
        .return_type
        .as_deref()
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    let displayed_return = if returns_self {
        class_name.to_string()
    } else if let Some(ref wrapper) = returns_wrapper_other {
        wrapper.clone()
    } else {
        match &idiom {
            ReturnIdiom::Plain => return_cs.clone(),
            ReturnIdiom::Option {
                payload_ty,
                ref_kind,
            } => {
                let raw = ref_kind_field_type(payload_ty, ref_kind, ir);
                let display = payload_display_cs(&raw, ir);
                // Value-type payloads (primitives + structs) become
                // `Nullable<T>`; wrapper-class payloads (`Dom?`) use the
                // same `T?` spelling as a nullable-reference annotation.
                // `string` is already a reference type.
                if display == "string" {
                    "string".to_string()
                } else {
                    format!("{}?", display)
                }
            }
            ReturnIdiom::Result {
                payload_ty,
                ref_kind,
            } => {
                let raw = ref_kind_field_type(payload_ty, ref_kind, ir);
                payload_display_cs(&raw, ir)
            }
        }
    };

    builder.line(&format!(
        "{} {} {}({})",
        modifiers.as_str(),
        displayed_return,
        method_name,
        arg_sig.join(", ")
    ));
    builder.line("{");
    builder.indent();

    if !is_static {
        builder.line("if (_disposed) throw new ObjectDisposedException(nameof(_inner));");
    }

    // Auto-string-conversion pre-call lines (see the rule predicate
    // above). Emitted before any of the call-emission branches so the
    // converted AzString locals are in scope for the call expression.
    for stmt in &pre_call_lines {
        builder.line(stmt);
    }

    // Use `managed_c_symbol(func)` (the C ABI symbol NativeMethods
    // declares — the `<c_name>Struct` triple-variant for functions
    // with callback-wrapper args). `func.method_name` is snake-case
    // from api.json and produces e.g. `AzFoo_with_resolver` instead
    // of the declared `AzFoo_withResolver`.
    let call = format!(
        "NativeMethods.{}({})",
        managed_c_symbol(func),
        call_args.join(", ")
    );

    // If the method receives `self`, produce an IntPtr to a heap-copy of
    // the FFI struct. Avoid `fixed`/`unsafe` so the same emit works under
    // PowerShell's Add-Type (no /unsafe option in PS 7's Roslyn wrapper).
    // Slight alloc cost per call; we copy back on return to mirror
    // mutation through `out` semantics.
    // Only emit the marshal path when self is taken by POINTER; for
    // by-value self we already pass `_inner` directly above.
    // App.Run blocks inside the native event loop for the app's lifetime;
    // flag that window so `Dispose(false)` (GC finalizer thread) can skip
    // cross-thread native deletes while it is open (see __AzAppLoopState).
    let is_app_run = func.c_name == "AzApp_run";
    if takes_self && !self_by_value {
        builder.line(&format!(
            "var __self = \
             System.Runtime.InteropServices.Marshal.AllocHGlobal(System.Runtime.InteropServices.\
             Marshal.SizeOf<{}>());",
            ffi_class_name
        ));
        builder.line("try");
        builder.line("{");
        builder.indent();
        builder.line(&"System.Runtime.InteropServices.Marshal.StructureToPtr(_inner, __self, false);".to_string());
        if is_app_run {
            builder.line("__AzAppLoopState.Running = true;");
        }
        let emit_cs_consume = |b: &mut CodeBuilder, names: &[String]| {
            for n in names {
                b.line(&format!("{}.__Consume();", n));
            }
        };
        if return_cs == "void" {
            builder.line(&format!("{};", call));
            builder.line(&format!(
                "_inner = System.Runtime.InteropServices.Marshal.PtrToStructure<{}>(__self);",
                ffi_class_name
            ));
            emit_cs_consume(builder, &consume_after_call);
        } else if returns_self {
            builder.line(&format!("var __raw = {};", call));
            builder.line(&format!(
                "_inner = System.Runtime.InteropServices.Marshal.PtrToStructure<{}>(__self);",
                ffi_class_name
            ));
            emit_cs_consume(builder, &consume_after_call);
            builder.line(&format!("return new {}(__raw);", class_name));
        } else if let Some(ref wrapper) = returns_wrapper_other {
            builder.line(&format!("var __raw = {};", call));
            builder.line(&format!(
                "_inner = System.Runtime.InteropServices.Marshal.PtrToStructure<{}>(__self);",
                ffi_class_name
            ));
            emit_cs_consume(builder, &consume_after_call);
            builder.line(&format!("return new {}(__raw);", wrapper));
        } else {
            builder.line(&format!("var __ret = {};", call));
            builder.line(&format!(
                "_inner = System.Runtime.InteropServices.Marshal.PtrToStructure<{}>(__self);",
                ffi_class_name
            ));
            emit_cs_consume(builder, &consume_after_call);
            match &idiom {
                ReturnIdiom::Plain => builder.line("return __ret;"),
                ReturnIdiom::Option {
                    payload_ty,
                    ref_kind,
                } => {
                    let raw = ref_kind_field_type(payload_ty, ref_kind, ir);
                    emit_cs_option_body(builder, &raw, &option_or_result_ty, ir);
                }
                ReturnIdiom::Result {
                    payload_ty,
                    ref_kind,
                } => {
                    let raw = ref_kind_field_type(payload_ty, ref_kind, ir);
                    emit_cs_result_body(builder, &raw, &option_or_result_ty, ir);
                }
            };
        }
        builder.dedent();
        builder.line("}");
        builder.line("finally");
        builder.line("{");
        builder.indent();
        if is_app_run {
            builder.line("__AzAppLoopState.Running = false;");
        }
        builder.line("System.Runtime.InteropServices.Marshal.FreeHGlobal(__self);");
        builder.dedent();
        builder.line("}");
    } else if takes_self && self_by_value {
        // By-value self: simple call, no Marshal needed.
        let emit_cs_consume = |b: &mut CodeBuilder, names: &[String]| {
            for n in names {
                b.line(&format!("{}.__Consume();", n));
            }
        };
        if return_cs == "void" {
            builder.line(&format!("{};", call));
            emit_cs_consume(builder, &consume_after_call);
        } else if returns_self {
            builder.line(&format!("var __raw = {};", call));
            emit_cs_consume(builder, &consume_after_call);
            builder.line(&format!("return new {}(__raw);", class_name));
        } else if let Some(ref wrapper) = returns_wrapper_other {
            builder.line(&format!("var __raw = {};", call));
            emit_cs_consume(builder, &consume_after_call);
            builder.line(&format!("return new {}(__raw);", wrapper));
        } else {
            match &idiom {
                ReturnIdiom::Plain => {
                    if consume_after_call.is_empty() {
                        builder.line(&format!("return {};", call));
                    } else {
                        builder.line(&format!("var __ret = {};", call));
                        emit_cs_consume(builder, &consume_after_call);
                        builder.line("return __ret;");
                    }
                }
                ReturnIdiom::Option {
                    payload_ty,
                    ref_kind,
                } => {
                    let raw = ref_kind_field_type(payload_ty, ref_kind, ir);
                    builder.line(&format!("var __ret = {};", call));
                    emit_cs_consume(builder, &consume_after_call);
                    emit_cs_option_body(builder, &raw, &option_or_result_ty, ir);
                }
                ReturnIdiom::Result {
                    payload_ty,
                    ref_kind,
                } => {
                    let raw = ref_kind_field_type(payload_ty, ref_kind, ir);
                    builder.line(&format!("var __ret = {};", call));
                    emit_cs_consume(builder, &consume_after_call);
                    emit_cs_result_body(builder, &raw, &option_or_result_ty, ir);
                }
            };
        }
    } else {
        let emit_cs_consume = |b: &mut CodeBuilder, names: &[String]| {
            for n in names {
                b.line(&format!("{}.__Consume();", n));
            }
        };
        if return_cs == "void" {
            builder.line(&format!("{};", call));
            emit_cs_consume(builder, &consume_after_call);
        } else if returns_self {
            builder.line(&format!("var __raw = {};", call));
            emit_cs_consume(builder, &consume_after_call);
            builder.line(&format!("return new {}(__raw);", class_name));
        } else if let Some(ref wrapper) = returns_wrapper_other {
            builder.line(&format!("var __raw = {};", call));
            emit_cs_consume(builder, &consume_after_call);
            builder.line(&format!("return new {}(__raw);", wrapper));
        } else {
            match &idiom {
                ReturnIdiom::Plain => {
                    if consume_after_call.is_empty() {
                        builder.line(&format!("return {};", call));
                    } else {
                        builder.line(&format!("var __ret = {};", call));
                        emit_cs_consume(builder, &consume_after_call);
                        builder.line("return __ret;");
                    }
                }
                ReturnIdiom::Option {
                    payload_ty,
                    ref_kind,
                } => {
                    let raw = ref_kind_field_type(payload_ty, ref_kind, ir);
                    builder.line(&format!("var __ret = {};", call));
                    emit_cs_consume(builder, &consume_after_call);
                    emit_cs_option_body(builder, &raw, &option_or_result_ty, ir);
                }
                ReturnIdiom::Result {
                    payload_ty,
                    ref_kind,
                } => {
                    let raw = ref_kind_field_type(payload_ty, ref_kind, ir);
                    builder.line(&format!("var __ret = {};", call));
                    emit_cs_consume(builder, &consume_after_call);
                    emit_cs_result_body(builder, &raw, &option_or_result_ty, ir);
                }
            };
        }
    }

    builder.dedent();
    builder.line("}");
    builder.blank();
}

// ============================================================================
// Tagged-union helper class
// ============================================================================

fn emit_union_helper(builder: &mut CodeBuilder, e: &EnumDef) {
    let class_name = sanitize_class_name(&e.name);
    let ffi_name = ffi_type_name(&e.name);
    let tag_name = format!("{}_Tag", ffi_name);

    if !e.doc.is_empty() {
        for d in &e.doc {
            builder.line(&format!("/// <summary>{}</summary>", xml_escape(d)));
        }
    }

    builder.line(&format!("public static class {}Helpers", class_name));
    builder.line("{");
    builder.indent();

    for v in &e.variants {
        match &v.kind {
            EnumVariantKind::Unit => {
                let pascal = snake_to_pascal(&v.name);
                builder.line(&format!(
                    "/// <summary>Construct the {}.{} variant.</summary>",
                    e.name, v.name
                ));
                builder.line(&format!("public static {} {}()", ffi_name, pascal));
                builder.line("{");
                builder.indent();
                builder.line(&format!("var u = new {}();", ffi_name));
                // Set tag on the variant's payload struct slot.
                let variant_field = sanitize_identifier(&v.name);
                builder.line(&format!(
                    "u.{}.tag = {}.{};",
                    variant_field,
                    tag_name,
                    sanitize_identifier(&v.name)
                ));
                builder.line("return u;");
                builder.dedent();
                builder.line("}");
                builder.blank();
            }
            EnumVariantKind::Tuple(_) | EnumVariantKind::Struct(_) => {
                // SKIPPED: payload-bearing variants need per-payload
                // overloads which depend on the FFI struct layout. The
                // user can construct these via the public FFI struct
                // fields directly.
                builder.line(&format!(
                    "// SKIPPED: variant {}.{} has payload — set fields directly on the FFI \
                     struct.",
                    e.name, v.name
                ));
                builder.blank();
            }
        }
    }

    builder.dedent();
    builder.line("}");
    builder.blank();
}

// ============================================================================
// Helpers
// ============================================================================

/// Pick a safe C# class name. We use the IR type name verbatim (no `Az`
/// prefix) because the wrapper lives inside `namespace Azul`.
fn sanitize_class_name(raw: &str) -> String {
    sanitize_identifier(raw)
}

/// Convert an api.json method name (typically already camelCase) to a
/// PascalCase C# method name, with a few special-casings.
fn idiomatic_method_name(method_name: &str) -> String {
    // Treat `new` specially — C# `new` is a keyword, surface it as
    // `Create` on the wrapper class.
    if method_name == "new" {
        return "Create".to_string();
    }

    // If it's already in lowerCamelCase or snake_case, normalise.
    if method_name.contains('_') {
        snake_to_pascal(method_name)
    } else {
        // Capitalise the first character.
        let mut chars = method_name.chars();
        match chars.next() {
            Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
            None => String::new(),
        }
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
