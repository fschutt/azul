//! Idiomatic wrapper-class emission for the Smalltalk generator.
//!
//! For every IR struct that has a matching `<TypeName>_delete` C
//! function, we emit a plain Smalltalk class `Azul<TypeName>` in the
//! `Azul-Core` package that:
//!
//! - Holds the raw `FFIExternalStructure` instance in an instance variable named `handle`.
//! - Registers itself with `WeakArray` / `WeakRegistry` finalization via
//!   `FFIExternalResourceManager addResource:`. When Pharo's GC reclaims the wrapper, `finalize` is
//!   invoked on a finalizer replacement that forwards to `AzulNative class >> az<Type>Delete:`.
//! - Surfaces every non-trait method on `<TypeName>` as an idiomatic instance or class-side method.
//!   Class-side static methods become factory selectors (`AzulApp create: anOptions`); instance
//!   methods forward `self handle` as the first FFI argument.
//!
//! Plain POD structs without a `_delete` get *no* wrapper — they are
//! used directly as `FFIExternalStructure` values.
//!
//! Tagged-union enums get a tiny helper class with a `Tag` accessor
//! and a static factory per unit variant; data-bearing variants are
//! left to direct field manipulation (same trade-off as the C# port).

use std::collections::BTreeSet;

use anyhow::Result;

use super::{
    super::{
        config::CodegenConfig,
        generator::CodeBuilder,
        ir::{
            CodegenIR, EnumDef, EnumVariantKind, FieldDef, FieldRefKind, FunctionDef, FunctionKind,
            StructDef, TypeCategory,
        },
        managed_host_invoker, managed_lang_helpers,
    },
    ffi_type_name, functions, map_type_to_uffi, method_category_line, sanitize_identifier,
    snake_to_lower_camel,
    types::class_header,
    wrapper_class_name, NATIVE_CLASS, PACKAGE_CORE,
};

// ============================================================================
// Public entry point (called from mod.rs)
// ============================================================================

pub fn generate_wrappers(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.line("\"---------------------------------------------------------------------------");
    builder.line(" Idiomatic wrapper classes (Azul-Core package).");
    builder.line(" Each wrapper holds a raw FFI handle and arranges for `finalize` to be");
    builder.line(" called by Pharo's GC, which forwards to AzulNative azXDelete:.");
    builder.line("---------------------------------------------------------------------------\"");
    builder.blank();

    emit_native_helpers(builder, ir, config);

    for s in &ir.structs {
        if !should_emit_struct_wrapper(s, ir, config) {
            continue;
        }
        emit_struct_wrapper(builder, s, ir, config);
    }

    for e in &ir.enums {
        if !should_emit_union_helper(e, config) {
            continue;
        }
        emit_union_helper(builder, e);
    }

    Ok(())
}

// ============================================================================
// Inclusion filters
// ============================================================================

fn should_emit_struct_wrapper(s: &StructDef, ir: &CodegenIR, config: &CodegenConfig) -> bool {
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

// ============================================================================
// Struct wrapper class
// ============================================================================

fn emit_struct_wrapper(
    builder: &mut CodeBuilder,
    s: &StructDef,
    ir: &CodegenIR,
    config: &CodegenConfig,
) {
    let class = wrapper_class_name(&s.name);
    let ffi_name = ffi_type_name(&s.name);

    class_header(
        builder,
        &class,
        "Object",
        &["handle"],
        &[],
        PACKAGE_CORE,
        &s.doc,
    );

    // ─── Instance-side accessors ──────────────────────────────────────────
    method_category_line(builder, "accessing");
    builder.line(&format!("{} >> handle [", class));
    builder.indent();
    builder.line("\"The underlying UnifiedFFI structure (read-only access for advanced use).\"");
    builder.line("^ handle");
    builder.dedent();
    builder.line("]");
    builder.blank();

    method_category_line(builder, "private");
    builder.line(&format!("{} >> setHandle: aHandle [", class));
    builder.indent();
    builder.line("handle := aHandle.");
    // Register with the finalization registry. Pharo's `WeakRegistry`
    // arranges for `finalize` to be sent when this wrapper is GC'd;
    // the handle itself survives the wrapper's death because it is
    // owned by the registry's executor.
    builder.line("self class finalizationRegistry add: self.");
    builder.line("^ self");
    builder.dedent();
    builder.line("]");
    builder.blank();

    // ─── Class-side construction helper ──────────────────────────────────
    method_category_line(builder, "private");
    builder.line(&format!("{} class >> wrap: aHandle [", class));
    builder.indent();
    builder.line("\"Wrap an existing AzulNative handle and arm the finalizer.\"");
    builder.line("aHandle isNil ifTrue: [ ^ nil ].");
    builder.line("^ self new setHandle: aHandle");
    builder.dedent();
    builder.line("]");
    builder.blank();

    method_category_line(builder, "finalization");
    builder.line(&format!("{} class >> finalizationRegistry [", class));
    builder.indent();
    builder.line("\"Lazily create a per-class WeakRegistry. Pharo's GC will send");
    builder.line(" `finalize` to each instance enrolled here when it is reclaimed.\"");
    builder.line("^ FinalizationRegistry default");
    builder.dedent();
    builder.line("]");
    builder.blank();

    // ─── finalize: invokes the C destructor exactly once ─────────────────
    let delete_selector = format!("az{}Delete:", to_pascal_local(&s.name));
    method_category_line(builder, "finalization");
    builder.line(&format!("{} >> finalize [", class));
    builder.indent();
    builder.line("\"Called automatically by Pharo's GC. Forwards to the C destructor.\"");
    builder.line("handle isNil ifTrue: [ ^ self ].");
    builder.line(&format!("{} {} handle.", NATIVE_CLASS, delete_selector));
    builder.line("handle := nil");
    builder.dedent();
    builder.line("]");
    builder.blank();

    // ─── azulConsume: hand the value over (a by-value argument) ──────────
    method_category_line(builder, "private");
    builder.line(&format!("{} >> azulConsume [", class));
    builder.indent();
    builder.line("\"The value moves into the callee: answer the handle and forget it, so");
    builder.line(" finalize can no longer free what the callee now owns.\"");
    builder.line("| h |");
    builder.line("h := handle.");
    builder.line("handle := nil.");
    builder.line("^ h");
    builder.dedent();
    builder.line("]");
    builder.blank();

    // ─── Methods: dispatch by FunctionKind ───────────────────────────────
    let mut taken: BTreeSet<String> = ["handle", "setHandle", "finalize", "azulConsume"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    for func in ir.functions_for_class(&s.name) {
        if func.kind.is_trait_function() {
            // Skip Delete/PartialEq/Cmp/Hash/Debug — finalize handles
            // delete and the others are not part of the wrapper API.
            continue;
        }
        taken.insert(idiomatic_method_name(&func.method_name));
        emit_wrapper_method(builder, &class, func, ir, config);
    }

    emit_field_accessors(builder, s, &class, ir, config, &taken);

    let _ = ffi_name; // reserved for future use (e.g., raw-handle accessors)
    builder.blank();
}

// ============================================================================
// Field accessors
// ============================================================================

/// The `AzulNative` primitive selector of `class`'s function of `kind`
/// (its first keyword, e.g. `azStringDelete:`), when that primitive exists.
fn native_selector(
    class: &str,
    kind: FunctionKind,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Option<String> {
    ir.functions_for_class(class.trim())
        .find(|f| f.kind == kind && functions::should_emit_function(f, ir, config))
        .map(|f| format!("{}:", snake_to_lower_camel(&f.c_name)))
}

/// Selectors every object answers: a field getter of the same name would
/// break the image's tools, so it gets a `Field` suffix instead.
fn is_object_protocol(sel: &str) -> bool {
    matches!(
        sel,
        "size"
            | "class"
            | "hash"
            | "value"
            | "copy"
            | "name"
            | "species"
            | "yourself"
            | "printString"
            | "displayString"
            | "error"
            | "halt"
            | "inspect"
            | "basicSize"
            | "shallowCopy"
            | "deepCopy"
            | "initialize"
            | "new"
            | "isNil"
            | "notNil"
            | "isString"
    )
}

/// How one field of a wrapped struct is read and written.
enum FieldShape {
    /// A C scalar, a unit enum, or a value that owns no heap memory: the
    /// compiled UnifiedFFI accessor reads (a copy) and writes it.
    Plain,
    /// The String class: read as a Smalltalk String, written from a
    /// String or a string value after the old one is released.
    Str { delete: String },
    /// A heap-owning value: read as a deep copy (`_clone`, wrapped when
    /// the type has a wrapper class), written by moving the new value in
    /// after the old one is released.
    Owning {
        wrapper: Option<String>,
        delete: String,
        clone: Option<String>,
    },
}

fn field_shape(f: &FieldDef, ir: &CodegenIR, config: &CodegenConfig) -> Option<FieldShape> {
    if !f.is_public || f.ref_kind != FieldRefKind::Owned {
        return None;
    }
    let t = f.type_name.trim();
    if t.contains('<') || t.starts_with('*') || t.starts_with('&') || t.starts_with('[') {
        return None;
    }
    // Callbacks, their wrappers and the type-erased handle are wired up by
    // the closure plumbing, never by writing bytes into a field.
    if managed_host_invoker::is_callback_wrapper(ir, t)
        || managed_lang_helpers::is_refany_type(t, ir)
        || ir.callback_typedefs.iter().any(|c| c.name.trim() == t)
    {
        return None;
    }
    let is_type =
        ir.find_struct(t).is_some() || ir.find_enum(t).is_some() || ir.find_type_alias(t).is_some();
    if is_type && !config.should_include_type(t) {
        return None;
    }
    if !managed_lang_helpers::has_delete_function(t, ir) {
        return Some(FieldShape::Plain);
    }
    // A heap-owning field without a reachable `_delete` cannot be replaced
    // without leaking the old value: no accessor at all.
    let delete = native_selector(t, FunctionKind::Delete, ir, config)?;
    if ir
        .find_struct(t)
        .is_some_and(|s| s.category == TypeCategory::String)
    {
        return Some(FieldShape::Str { delete });
    }
    Some(FieldShape::Owning {
        wrapper: ir
            .find_struct(t)
            .filter(|s| should_emit_struct_wrapper(s, ir, config))
            .map(|s| wrapper_class_name(&s.name)),
        delete,
        clone: native_selector(t, FunctionKind::DeepCopy, ir, config),
    })
}

/// `<field>` / `<field>:` for every public by-value field of a wrapped
/// struct, on top of the UnifiedFFI accessors `fieldsDesc` compiles:
///
/// - a getter answers an independent value - a String field decodes to a
///   Smalltalk String without consuming it, a heap-owning field is a deep
///   copy (`_clone`, wrapped so it is finalized; none -> no getter), any
///   other field is the accessor's copy;
/// - a setter releases the field's old value (`_delete`), then moves the
///   new one in: a wrapper argument is consumed (`azulConsume`, so its
///   finalizer cannot free it again), a Smalltalk String becomes a fresh
///   AzString.
///
/// Getters answer COPIES, so a nested write is read-modify-write:
/// `ws := opts windowState. ws title: 'Hi'. opts windowState: ws`.
///
/// An api.json method of the same selector wins over the getter; the
/// setter is still emitted (it is a different selector).
fn emit_field_accessors(
    builder: &mut CodeBuilder,
    s: &StructDef,
    class: &str,
    ir: &CodegenIR,
    config: &CodegenConfig,
    taken: &BTreeSet<String>,
) {
    let mut first = true;
    for f in &s.fields {
        let Some(shape) = field_shape(f, ir, config) else {
            continue;
        };
        let slot = sanitize_identifier(&f.name);
        let mut sel = snake_to_lower_camel(&f.name);
        if is_object_protocol(&sel) {
            sel.push_str("Field");
        }
        if first {
            first = false;
            builder.line("\"Field accessors answer COPIES; a nested write is read-modify-write:");
            builder.line(" ws := opts windowState. ws title: 'Hi'. opts windowState: ws.");
            builder.line(" A setter releases the old value and MOVES the new one in.\"");
        }
        let getter: Option<String> = match &shape {
            FieldShape::Plain => Some(format!("^ handle {}", slot)),
            FieldShape::Str { .. } => Some(format!(
                "^ {} stringFromAzString: handle {}",
                NATIVE_CLASS, slot
            )),
            FieldShape::Owning {
                wrapper: Some(w),
                clone: Some(c),
                ..
            } => Some(format!(
                "^ {} wrap: ({} {} handle {})",
                w, NATIVE_CLASS, c, slot
            )),
            FieldShape::Owning {
                wrapper: None,
                clone: Some(c),
                ..
            } => Some(format!("^ {} {} handle {}", NATIVE_CLASS, c, slot)),
            FieldShape::Owning { clone: None, .. } => None,
        };
        if let Some(body) = getter {
            if !taken.contains(&sel) {
                method_category_line(builder, "fields");
                builder.line(&format!("{} >> {} [", class, sel));
                builder.indent();
                builder.line(&format!("\"A copy of the `{}` field.\"", f.name));
                builder.line(&body);
                builder.dedent();
                builder.line("]");
                builder.blank();
            }
        }
        method_category_line(builder, "fields");
        builder.line(&format!("{} >> {}: aValue [", class, sel));
        builder.indent();
        builder.line(&format!(
            "\"Replace the `{}` field; the old value is released, aValue is moved in.\"",
            f.name
        ));
        match &shape {
            FieldShape::Plain => builder.line(&format!("handle {}: aValue", slot)),
            FieldShape::Str { delete } => {
                builder.line("| new |");
                builder.line(&format!(
                    "new := aValue isString ifTrue: [ {n} azStringFrom: aValue ] ifFalse: [ ({n} azulMove: aValue) ].",
                    n = NATIVE_CLASS
                ));
                builder.line(&format!("{} {} handle {}.", NATIVE_CLASS, delete, slot));
                builder.line(&format!("handle {}: new", slot));
            }
            FieldShape::Owning { delete, .. } => {
                builder.line("| new |");
                builder.line(&format!("new := ({} azulMove: aValue).", NATIVE_CLASS));
                builder.line(&format!("{} {} handle {}.", NATIVE_CLASS, delete, slot));
                builder.line(&format!("handle {}: new", slot));
            }
        }
        builder.dedent();
        builder.line("]");
        builder.blank();
    }
}

// ============================================================================
// AzulNative helpers: moving wrappers, String conversion
// ============================================================================

/// Class-side helpers on `AzulNative` the wrappers call:
///
/// - `azulMove:` - a by-value argument: a wrapper is consumed (its handle
///   answered and forgotten, so its finalizer cannot free the bytes the
///   callee now owns), a Smalltalk String becomes a fresh AzString,
///   anything else (a raw structure) passes through;
/// - `azulBorrow:` - a by-pointer argument: a wrapper's handle, else as is;
/// - `azStringFrom:` / `stringFromAzString:` - String conversion; the
///   decoder only reads the bytes (field names from the IR).
fn emit_native_helpers(builder: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    let n = NATIVE_CLASS;
    method_category_line(builder, "azul-helpers");
    builder.line(&format!("{} class >> azulMove: aValue [", n));
    builder.indent();
    builder.line("\"A by-value argument: a wrapper hands its value over (and will not free it).\"");
    builder.line("aValue isString ifTrue: [ ^ self azStringFrom: aValue ].");
    builder.line("^ (aValue respondsTo: #azulConsume)");
    builder.line("    ifTrue: [ aValue azulConsume ]");
    builder.line("    ifFalse: [ aValue ]");
    builder.dedent();
    builder.line("]");
    builder.blank();

    method_category_line(builder, "azul-helpers");
    builder.line(&format!("{} class >> azulBorrow: aValue [", n));
    builder.indent();
    builder.line("\"A by-pointer argument: a wrapper lends its handle.\"");
    builder.line("^ (aValue respondsTo: #azulConsume)");
    builder.line("    ifTrue: [ aValue handle ]");
    builder.line("    ifFalse: [ aValue ]");
    builder.dedent();
    builder.line("]");
    builder.blank();

    // The String class, its byte constructor, and where its bytes are
    // described: all read off the IR.
    let Some(string) = ir
        .structs
        .iter()
        .find(|s| s.category == TypeCategory::String)
    else {
        return;
    };
    let ctor = ir
        .functions_for_class(&string.name)
        // allow-api-name: no kind or shape distinguishes this constructor.
        .find(|f| f.method_name == "copy_from_bytes")
        .filter(|f| functions::should_emit_function(f, ir, config) && f.args.len() == 3);
    if let Some(ctor) = ctor {
        method_category_line(builder, "azul-helpers");
        builder.line(&format!("{} class >> azStringFrom: aString [", n));
        builder.indent();
        builder.line("\"A fresh AzString holding a UTF-8 copy of aString.\"");
        builder.line("| bytes |");
        builder.line("bytes := aString utf8Encoded.");
        builder.line(&format!(
            "^ self {}: bytes {}: 0 {}: bytes size",
            snake_to_lower_camel(&ctor.c_name),
            ctor.args[1].name,
            ctor.args[2].name
        ));
        builder.dedent();
        builder.line("]");
        builder.blank();
    }
    let vec = string.fields.first();
    let inner = vec.and_then(|v| ir.find_struct(v.type_name.trim()));
    let ptr = inner.and_then(|i| {
        i.fields
            .iter()
            .find(|f| f.ref_kind != FieldRefKind::Owned || f.type_name.trim().starts_with('*'))
    });
    let len = inner.and_then(|i| i.fields.iter().find(|f| f.type_name.trim() == "usize"));
    if let (Some(vec), Some(ptr), Some(len)) = (vec, ptr, len) {
        method_category_line(builder, "azul-helpers");
        builder.line(&format!("{} class >> stringFromAzString: anAzString [", n));
        builder.indent();
        builder.line("\"The Smalltalk String held by anAzString (a structure or a wrapper),");
        builder.line(" which is only read, never consumed.\"");
        builder.line("| v ptr len bytes addr |");
        builder.line(&format!(
            "v := (self azulBorrow: anAzString) {}.",
            sanitize_identifier(&vec.name)
        ));
        builder.line(&format!("ptr := v {}.", sanitize_identifier(&ptr.name)));
        builder.line(&format!("len := v {}.", sanitize_identifier(&len.name)));
        builder.line("(len isNil or: [ len = 0 ]) ifTrue: [ ^ '' ].");
        builder.line("addr := ptr isExternalAddress ifTrue: [ ptr ] ifFalse: [ ptr getHandle ].");
        builder.line("bytes := ByteArray new: len.");
        builder.line("1 to: len do: [ :i | bytes at: i put: (addr byteAt: i) ].");
        builder.line("^ bytes utf8Decoded");
        builder.dedent();
        builder.line("]");
        builder.blank();
    }
}

// ============================================================================
// Wrapper method emission
// ============================================================================

fn emit_wrapper_method(
    builder: &mut CodeBuilder,
    class: &str,
    func: &FunctionDef,
    ir: &CodegenIR,
    config: &CodegenConfig,
) {
    let return_type = func
        .return_type
        .as_ref()
        .map(|r| map_type_to_uffi(r, ir))
        .unwrap_or_else(|| "void".to_string());

    let returns_self = func
        .return_type
        .as_deref()
        .map(|r| r.trim() == func.class_name)
        .unwrap_or(false);

    let user_args: Vec<_> = func
        .args
        .iter()
        .filter(|a| !func.is_receiver_arg(a))
        .collect();

    let is_static = matches!(
        func.kind,
        FunctionKind::Constructor | FunctionKind::StaticMethod | FunctionKind::Default
    );

    // Build the wrapper-side selector. Strip any `<TypeName>_` prefix
    // off the C method name and convert the remainder to lowerCamel.
    let method_base = idiomatic_method_name(&func.method_name);

    // Emit method header.
    if !func.doc.is_empty() {
        for d in &func.doc {
            builder.line(&format!("\"{}\"", d.replace('"', "''")));
        }
    }
    method_category_line(
        builder,
        if is_static {
            "instance creation"
        } else {
            "api"
        },
    );

    let receiver = if is_static {
        format!("{} class", class)
    } else {
        class.to_string()
    };

    // Build wrapper-side selector with keyword args.
    let mut sel = String::new();
    if user_args.is_empty() {
        sel.push_str(&method_base);
    } else {
        for (i, a) in user_args.iter().enumerate() {
            let id = sanitize_identifier(&a.name);
            if i == 0 {
                sel.push_str(&format!("{}: {}", method_base, id));
            } else {
                sel.push_str(&format!(" {}: {}", a.name, id));
            }
        }
    }

    builder.line(&format!("{} >> {} [", receiver, sel));
    builder.indent();

    // Build the AzulNative call. The class-side primitive selector
    // mirrors `func.c_name`. The C name is e.g. `AzApp_create`; the
    // primitive selector built in functions.rs is the snake-to-camel
    // form. Reproduce that here.
    let prim_base = snake_to_lower_camel(&func.c_name);

    // Construct the primitive selector with its keyword args.
    // Argument list: handle (if instance method) followed by user args.
    let mut prim_args: Vec<(String, String)> = Vec::new(); // (keyword, expression)
    let takes_self = matches!(
        func.kind,
        FunctionKind::Method | FunctionKind::MethodMut | FunctionKind::DeepCopy
    );

    let mut first_arg_name: Option<String> = None;
    if takes_self {
        // Find the implicit `self` arg name in the FFI signature so we
        // can use the same keyword as the primitive method declares.
        if let Some(a) = func.args.first() {
            first_arg_name = Some(a.name.clone());
        }
    }

    for a in &func.args {
        let id = sanitize_identifier(&a.name);
        if takes_self
            && first_arg_name.as_deref() == Some(a.name.as_str())
            && func.is_receiver_arg(a)
        {
            prim_args.push((a.name.clone(), "handle".to_string()));
        } else {
            // A wrapped class (or a String) is unwrapped: by value it is
            // MOVED (`azulMove:` consumes the wrapper, so its finalizer
            // cannot free what the callee now owns - `run:` used to free
            // the window options twice), by pointer it is lent.
            let wrapped = ir
                .find_struct(a.type_name.trim())
                .is_some_and(|s| should_emit_struct_wrapper(s, ir, config));
            let expr = match (wrapped, a.ref_kind) {
                (true, super::super::ir::ArgRefKind::Owned) => {
                    format!("({} azulMove: {})", NATIVE_CLASS, id)
                }
                (true, _) => format!("({} azulBorrow: {})", NATIVE_CLASS, id),
                (false, _) => id,
            };
            prim_args.push((a.name.clone(), expr));
        }
    }

    let mut prim_call = String::new();
    if prim_args.is_empty() {
        prim_call.push_str(&format!("{} {}", NATIVE_CLASS, prim_base));
    } else {
        for (i, (kw, expr)) in prim_args.iter().enumerate() {
            if i == 0 {
                prim_call.push_str(&format!("{} {}: {}", NATIVE_CLASS, prim_base, expr));
            } else {
                prim_call.push_str(&format!(" {}: {}", kw, expr));
            }
        }
    }

    // Consume-after-by-value: when the C ABI took `handle` by value
    // (DeepCopy / consuming-self), Rust owns those bytes. Nil out
    // `handle` so the WeakRegistry finalizer's `handle isNil`
    // short-circuit skips on cleanup. Mirrors Pascal/Fortran
    // FOwned/owned-flag and JVM `__consume()` patterns.
    let self_by_value = takes_self
        && func
            .args
            .first()
            .map(|a| matches!(a.ref_kind, super::super::ir::ArgRefKind::Owned))
            .unwrap_or(false);

    // Decide what to do with the primitive return value.
    if return_type == "void" {
        builder.line(&format!("{}.", prim_call));
        if self_by_value {
            builder.line("handle := nil.");
        }
        builder.line("^ self");
    } else if returns_self && is_static {
        builder.line(&format!("^ self wrap: ({})", prim_call));
    } else if returns_self && !is_static {
        // Instance method that returns the same type — wrap the new
        // handle in a fresh wrapper instance. If self_by_value, mark
        // the old wrapper consumed before constructing the new one.
        if self_by_value {
            builder.line(&"| _ret |".to_string());
            builder.line(&format!("_ret := self class wrap: ({}).", prim_call));
            builder.line("handle := nil.");
            builder.line("^ _ret");
        } else {
            builder.line(&format!("^ self class wrap: ({})", prim_call));
        }
    } else {
        if self_by_value {
            builder.line(&"| _ret |".to_string());
            builder.line(&format!("_ret := {}.", prim_call));
            builder.line("handle := nil.");
            builder.line("^ _ret");
        } else {
            builder.line(&format!("^ {}", prim_call));
        }
    }

    builder.dedent();
    builder.line("]");
    builder.blank();
}

// ============================================================================
// Tagged-union helper
// ============================================================================

fn emit_union_helper(builder: &mut CodeBuilder, e: &EnumDef) {
    let class = format!("{}Helpers", wrapper_class_name(&e.name));
    let ffi_name = ffi_type_name(&e.name);
    let tag_name = format!("{}_Tag", ffi_name);

    class_header(builder, &class, "Object", &[], &[], PACKAGE_CORE, &e.doc);

    for v in &e.variants {
        match &v.kind {
            EnumVariantKind::Unit => {
                let factory = idiomatic_method_name(&v.name);
                method_category_line(builder, "instance creation");
                builder.line(&format!("{} class >> {} [", class, factory));
                builder.indent();
                builder.line(&format!(
                    "\"Construct the {}.{} variant — unit (no payload).\"",
                    e.name, v.name
                ));
                builder.line(&"| u |".to_string());
                builder.line(&format!("u := {} new.", ffi_name));
                // A `repr(C, u8)` tag field is a `uint8` (types.rs), so it
                // takes the enumeration's integer value.
                let value = if e.repr.as_deref().is_some_and(|r| r.contains("u8")) {
                    " value"
                } else {
                    ""
                };
                builder.line(&format!(
                    "(u {}) tag: ({} {}){}.",
                    sanitize_identifier(&v.name),
                    tag_name,
                    sanitize_identifier(&v.name),
                    value
                ));
                builder.line("^ u");
                builder.dedent();
                builder.line("]");
                builder.blank();
            }
            EnumVariantKind::Tuple(_) | EnumVariantKind::Struct(_) => {
                builder.line(&format!(
                    "\"SKIPPED: {}.{} carries a payload — set fields directly on the FFI struct.\"",
                    e.name, v.name
                ));
                builder.blank();
            }
        }
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Strip the leading `<ClassName>_` from a method name (if any) and
/// convert what remains to lowerCamelCase. Treats the keyword
/// `new` as the customary Smalltalk `create` to avoid clashing with
/// `Object class >> new`.
fn idiomatic_method_name(method_name: &str) -> String {
    if method_name == "new" {
        return "create".to_string();
    }
    snake_to_lower_camel(method_name)
}

/// Convert "App" -> "App", "double_buffer" -> "DoubleBuffer". Local
/// helper for forming `azXDelete:` style primitive names where the
/// first character must remain uppercase to match the Az-prefixed
/// FFI symbol.
fn to_pascal_local(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut upper = true;
    for c in s.chars() {
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
}

#[cfg(test)]
mod tests {
    use super::super::super::{bug_classes::ir, config::CodegenConfig};

    fn st() -> &'static str {
        static OUT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
        OUT.get_or_init(|| super::super::generate(ir(), &CodegenConfig::c_header()).unwrap())
    }

    /// The text of the method (or class) starting with `head`, up to the
    /// closing `]` at column 0.
    fn method(head: &str) -> &'static str {
        let out = st();
        let start = out
            .find(head)
            .unwrap_or_else(|| panic!("no `{head}` in Azul.st"));
        let rest = &out[start..];
        let end = rest.find("\n]").map_or(rest.len(), |e| e + 2);
        &rest[..end]
    }

    #[test]
    fn a_struct_layout_is_a_fields_desc_that_unified_ffi_reads() {
        assert!(!st().contains("AzFullWindowState class >> fields ["));
        let desc = method("AzFullWindowState class >> fieldsDesc [");
        assert!(desc.contains("AzString title;"), "{desc}");
        assert!(desc.contains("bool window_focused;"), "{desc}");
    }

    #[test]
    fn struct_classes_compile_their_field_accessors_on_load() {
        let init = method("AzFullWindowState class >> initialize [");
        assert!(init.contains("self compileFields"), "{init}");
    }

    #[test]
    fn the_title_getter_decodes_the_string_without_consuming_it() {
        let get = method("AzulFullWindowState >> title [");
        assert!(
            get.contains("^ AzulNative stringFromAzString: handle title"),
            "{get}"
        );
        let decode = method("AzulNative class >> stringFromAzString: anAzString [");
        assert!(decode.contains("utf8Decoded"), "{decode}");
    }

    #[test]
    fn the_title_setter_releases_the_old_string_then_stores_the_new_one() {
        let set = method("AzulFullWindowState >> title: aValue [");
        let delete = set
            .find("AzulNative azStringDelete: handle title.")
            .expect(set);
        let store = set.find("handle title: ").expect(set);
        assert!(delete < store, "{set}");
        assert!(set.contains("AzulNative azStringFrom: aValue"), "{set}");
    }

    #[test]
    fn the_window_state_getter_deep_copies_and_the_setter_deletes_then_consumes() {
        let get = method("AzulWindowCreateOptions >> windowState [");
        assert!(
            get.contains(
                "^ AzulFullWindowState wrap: (AzulNative azFullWindowStateClone: handle window_state)"
            ),
            "{get}"
        );
        let set = method("AzulWindowCreateOptions >> windowState: aValue [");
        assert!(
            set.contains("AzulNative azFullWindowStateDelete: handle window_state."),
            "{set}"
        );
        assert!(set.contains("(AzulNative azulMove: aValue)"), "{set}");
    }

    #[test]
    fn running_the_app_consumes_the_window_options_so_their_finalizer_cannot_free_them_again() {
        let run = method("AzulApp >> run: root_window [");
        assert!(run.contains("(AzulNative azulMove: root_window)"), "{run}");
        let mv = method("AzulNative class >> azulMove: aValue [");
        assert!(mv.contains("azulConsume"), "{mv}");
    }

    #[test]
    fn the_text_field_stays_writable_next_to_the_get_text_method() {
        assert!(st().contains("AzulTextInputState >> getText ["));
        let set = method("AzulTextInputState >> text: aValue [");
        assert!(
            set.contains("AzulNative azU32VecDelete: handle text."),
            "{set}"
        );
    }
}
