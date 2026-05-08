//! Idiomatic wrapper-class emission for the Smalltalk generator.
//!
//! For every IR struct that has a matching `<TypeName>_delete` C
//! function, we emit a plain Smalltalk class `Azul<TypeName>` in the
//! `Azul-Core` package that:
//!
//! - Holds the raw `FFIExternalStructure` instance in an instance
//!   variable named `handle`.
//! - Registers itself with `WeakArray` / `WeakRegistry` finalization
//!   via `FFIExternalResourceManager addResource:`. When Pharo's GC
//!   reclaims the wrapper, `finalize` is invoked on a finalizer
//!   replacement that forwards to `AzulNative class >> az<Type>Delete:`.
//! - Surfaces every non-trait method on `<TypeName>` as an idiomatic
//!   instance or class-side method. Class-side static methods become
//!   factory selectors (`AzulApp create: anOptions`); instance methods
//!   forward `self handle` as the first FFI argument.
//!
//! Plain POD structs without a `_delete` get *no* wrapper — they are
//! used directly as `FFIExternalStructure` values.
//!
//! Tagged-union enums get a tiny helper class with a `Tag` accessor
//! and a static factory per unit variant; data-bearing variants are
//! left to direct field manipulation (same trade-off as the C# port).

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{
    CodegenIR, EnumDef, EnumVariantKind, FunctionDef, FunctionKind, StructDef, TypeCategory,
};
use super::types::class_header;
use super::{
    ffi_type_name, map_type_to_uffi, method_category_line, sanitize_identifier,
    snake_to_lower_camel, wrapper_class_name, NATIVE_CLASS, PACKAGE_CORE,
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

    for s in &ir.structs {
        if !should_emit_struct_wrapper(s, ir, config) {
            continue;
        }
        emit_struct_wrapper(builder, s, ir);
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

fn emit_struct_wrapper(builder: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR) {
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

    // ─── Methods: dispatch by FunctionKind ───────────────────────────────
    for func in ir.functions_for_class(&s.name) {
        if func.kind.is_trait_function() {
            // Skip Delete/PartialEq/Cmp/Hash/Debug — finalize handles
            // delete and the others are not part of the wrapper API.
            continue;
        }
        emit_wrapper_method(builder, &class, func, ir);
    }

    let _ = ffi_name; // reserved for future use (e.g., raw-handle accessors)
    builder.blank();
}

// ============================================================================
// Wrapper method emission
// ============================================================================

fn emit_wrapper_method(
    builder: &mut CodeBuilder,
    class: &str,
    func: &FunctionDef,
    ir: &CodegenIR,
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

    let class_lower = func.class_name.to_lowercase();
    let user_args: Vec<_> = func
        .args
        .iter()
        .filter(|a| a.name != class_lower && a.name != "self")
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
    method_category_line(builder, if is_static { "instance creation" } else { "api" });

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
            && (a.name == "self" || a.name == class_lower)
        {
            prim_args.push((a.name.clone(), "handle".to_string()));
        } else {
            prim_args.push((a.name.clone(), id));
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

    // Decide what to do with the primitive return value.
    if return_type == "void" {
        builder.line(&format!("{}.", prim_call));
        builder.line("^ self");
    } else if returns_self && is_static {
        builder.line(&format!("^ self wrap: ({})", prim_call));
    } else if returns_self && !is_static {
        // Instance method that returns the same type — wrap the new
        // handle in a fresh wrapper instance.
        builder.line(&format!("^ self class wrap: ({})", prim_call));
    } else {
        builder.line(&format!("^ {}", prim_call));
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
                builder.line(&format!("| u |"));
                builder.line(&format!("u := {} new.", ffi_name));
                builder.line(&format!(
                    "(u {}) tag: ({} {}).",
                    sanitize_identifier(&v.name),
                    tag_name,
                    sanitize_identifier(&v.name)
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

