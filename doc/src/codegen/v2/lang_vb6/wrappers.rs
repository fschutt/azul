//! VB6 Class Module (`.cls`) emission.
//!
//! For every IR struct that has a matching `<TypeName>_delete` C
//! function, we emit a VB6 Class Module (`.cls` file). The class:
//!
//! 1. Holds the underlying FFI record (`AzTypeName`) by value in a
//!    private `m_raw` field, plus an `m_owned` flag so wrap-existing
//!    factories can opt out of automatic deletion.
//! 2. Exposes one or more `Public Sub Init...(...)` initialisers per
//!    IR `FunctionKind::Constructor` / `FunctionKind::Default` method
//!    on the type. VB6 does **not** support overloaded
//!    constructors, so each constructor gets a distinct `Init<Suffix>`
//!    name.
//! 3. Implements `Class_Initialize` / `Class_Terminate` — the latter
//!    calls the matching `_delete` extern when `m_owned` is True.
//! 4. Surfaces every non-trait method as a `Public Function`/`Sub`
//!    delegating to the FFI symbol with `m_raw` passed as the
//!    self-pointer.
//!
//! User-facing class names drop the `Az` prefix:  `AzApp` → `App`,
//! `AzWindow` → `Window`. Each lives in its own `.cls` file.
//!
//! # Class file shape
//!
//! VB6 `.cls` files have a fixed seven-line preamble that tells the
//! IDE this is a class module. Mess that up and the IDE rejects the
//! file. The preamble is:
//!
//! ```text
//! VERSION 1.0 CLASS
//! BEGIN
//!   MultiUse = -1  'True
//!   Persistable = 0  'NotPersistable
//!   DataBindingBehavior = 0  'vbNone
//!   DataSourceBehavior  = 0  'vbNone
//!   MTSTransactionMode  = 0  'NotAnMTSObject
//! END
//! Attribute VB_Name = "<ClassName>"
//! Attribute VB_GlobalNameSpace = False
//! Attribute VB_Creatable = True
//! Attribute VB_PredeclaredId = False
//! Attribute VB_Exposed = True
//! ```

use anyhow::Result;
use std::collections::BTreeSet;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{
    ArgRefKind, CodegenIR, FunctionArg, FunctionDef, FunctionKind, StructDef, TypeCategory,
};
use super::{
    ffi_type_name, idiomatic_method_name, map_type_to_vb6, sanitize_comment, sanitize_identifier,
};

// ============================================================================
// Discovery
// ============================================================================

/// Collect every struct that should be wrapped in a `.cls` Class
/// Module — i.e. every struct with a matching `_delete` extern.
pub fn collect_class_targets<'a>(
    ir: &'a CodegenIR,
    config: &CodegenConfig,
) -> Vec<&'a StructDef> {
    let delete_set: BTreeSet<&str> = ir
        .functions
        .iter()
        .filter(|f| f.kind == FunctionKind::Delete)
        .map(|f| f.class_name.as_str())
        .collect();

    ir.structs
        .iter()
        .filter(|s| should_emit_wrapper(s, config) && delete_set.contains(s.name.as_str()))
        .collect()
}

fn should_emit_wrapper(s: &StructDef, config: &CodegenConfig) -> bool {
    if !config.should_include_type(&s.name) {
        return false;
    }
    if !s.generic_params.is_empty() {
        return false;
    }
    !matches!(
        s.category,
        TypeCategory::Recursive
            | TypeCategory::VecRef
            | TypeCategory::DestructorOrClone
            | TypeCategory::GenericTemplate
    )
}

/// Idiomatic class name — drop the `Az` prefix from the IR name.
/// `App` -> `App`, `Window` -> `Window`. (The IR already strips
/// `Az`; this keeps the helper symmetric with other generators
/// in case the IR-level convention changes.)
pub fn class_name_for(raw: &str) -> String {
    raw.strip_prefix("Az").unwrap_or(raw).to_string()
}

// ============================================================================
// Class module emission
// ============================================================================

/// Build the `.cls` body for a single disposable type. Returned string
/// is what the orchestrator writes to `<ClassName>.cls`.
pub fn emit_class_module(
    s: &StructDef,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<String> {
    let mut builder = CodeBuilder::new(&config.indent);
    let class_name = class_name_for(&s.name);
    let raw_record = ffi_type_name(&s.name);

    // VB6 .cls preamble (verbatim — the IDE parses this).
    emit_preamble(&mut builder, &class_name);
    builder.blank();

    builder.line("Option Explicit");
    builder.blank();

    // Doc block.
    if !s.doc.is_empty() {
        for d in &s.doc {
            builder.line(&format!("' {}", sanitize_comment(d)));
        }
    } else {
        builder.line(&format!(
            "' Idiomatic VB6 wrapper for {}. Class_Terminate calls {}_delete.",
            raw_record, raw_record
        ));
    }
    builder.blank();

    // Private state.
    builder.line(&format!("Private m_raw As {}", raw_record));
    builder.line("Private m_owned As Boolean");
    builder.blank();

    // Class_Initialize: VB6 fires this automatically when the class is
    // instantiated. We default `m_owned` to False so a fresh class
    // doesn't try to delete an unitialised m_raw at termination —
    // the user must call one of the InitXxx sub-initialisers (or
    // WrapRaw) to populate m_raw and flip m_owned to True.
    builder.line("Private Sub Class_Initialize()");
    builder.indent();
    builder.line("m_owned = False");
    builder.dedent();
    builder.line("End Sub");
    builder.blank();

    // Class_Terminate: the destructor hook — VB6 fires it automatically
    // when the last reference to the class instance drops.
    builder.line("Private Sub Class_Terminate()");
    builder.indent();
    builder.line("If m_owned Then");
    builder.indent();
    builder.line(&format!("{}_delete VarPtr(m_raw)", raw_record));
    builder.line("m_owned = False");
    builder.dedent();
    builder.line("End If");
    builder.dedent();
    builder.line("End Sub");
    builder.blank();

    // Wrap-existing factory: takes a Long pointer to a populated FFI
    // record and copies its bytes into m_raw, claiming ownership.
    builder.line("' WrapRaw: take ownership of an existing AzXxx record (passed via VarPtr).");
    builder.line(&format!("Public Sub WrapRaw(ByVal rawPtr As Long)"));
    builder.indent();
    builder.line(&format!(
        "CopyMemory m_raw, ByVal rawPtr, LenB(m_raw)"
    ));
    builder.line("m_owned = True");
    builder.dedent();
    builder.line("End Sub");
    builder.blank();

    // Raw-pointer accessor (escape hatch).
    builder.line("' GetRawPtr: returns a Long-as-pointer to the underlying AzXxx record.");
    builder.line("' Use this to pass `this` to externals that take an AzXxx pointer (ByVal Long).");
    builder.line("Public Function GetRawPtr() As Long");
    builder.indent();
    builder.line("GetRawPtr = VarPtr(m_raw)");
    builder.dedent();
    builder.line("End Function");
    builder.blank();

    // Constructors / Default → InitXxx subs.
    let mut init_index = 0usize;
    for func in ir.functions_for_class(&s.name) {
        if !matches!(func.kind, FunctionKind::Constructor | FunctionKind::Default) {
            continue;
        }
        emit_init_sub(&mut builder, &raw_record, func, ir, init_index);
        init_index += 1;
    }

    // Instance / static methods.
    for func in ir.functions_for_class(&s.name) {
        if matches!(
            func.kind,
            FunctionKind::Constructor | FunctionKind::Default | FunctionKind::Delete
        ) {
            continue;
        }
        if func.kind.is_trait_function() {
            continue;
        }
        emit_method(&mut builder, &raw_record, func, ir);
    }

    Ok(builder.finish())
}

fn emit_preamble(builder: &mut CodeBuilder, class_name: &str) {
    builder.line("VERSION 1.0 CLASS");
    builder.line("BEGIN");
    builder.line("  MultiUse = -1  'True");
    builder.line("  Persistable = 0  'NotPersistable");
    builder.line("  DataBindingBehavior = 0  'vbNone");
    builder.line("  DataSourceBehavior  = 0  'vbNone");
    builder.line("  MTSTransactionMode  = 0  'NotAnMTSObject");
    builder.line("END");
    builder.line(&format!("Attribute VB_Name = \"{}\"", class_name));
    builder.line("Attribute VB_GlobalNameSpace = False");
    builder.line("Attribute VB_Creatable = True");
    builder.line("Attribute VB_PredeclaredId = False");
    builder.line("Attribute VB_Exposed = True");
}

// ============================================================================
// Init<X> sub-initialiser per Constructor / Default.
// ============================================================================
//
// VB6 has no overloaded constructors. We name the first constructor
// `Init`; subsequent ones get the C method name appended to disambiguate
// (`InitNew`, `InitDefault`, etc.).

fn emit_init_sub(
    builder: &mut CodeBuilder,
    raw_record: &str,
    func: &FunctionDef,
    ir: &CodegenIR,
    index: usize,
) {
    let suffix = if index == 0 {
        String::new()
    } else {
        idiomatic_method_name(&func.method_name)
    };
    let init_name = if suffix.is_empty() {
        "Init".to_string()
    } else {
        format!("Init{}", suffix)
    };

    let visible = visible_user_args(func);
    let args_str = format_arg_list(&visible, ir);

    if !func.doc.is_empty() {
        for d in &func.doc {
            builder.line(&format!("' {}", sanitize_comment(d)));
        }
    } else {
        builder.line(&format!(
            "' {}: initialise via {}. Sets ownership flag.",
            init_name, func.c_name
        ));
    }

    if args_str.is_empty() {
        builder.line(&format!("Public Sub {}()", init_name));
    } else {
        builder.line(&format!("Public Sub {}({})", init_name, args_str));
    }
    builder.indent();

    let returns_self = func
        .return_type
        .as_deref()
        .map(|r| r.trim() == func.class_name)
        .unwrap_or(false);

    let call_args: Vec<String> = visible
        .iter()
        .map(|a| sanitize_identifier(&a.name))
        .collect();
    let call = format!("{}({})", func.c_name, call_args.join(", "));

    if returns_self {
        // The C function returns the AzXxx record by value — but VB6
        // Declare cannot return UDTs by value (see functions.rs).
        // SKIPPED: real assignment uses CopyMemory from the C-shim
        // out-pointer; here we record the limitation in a comment.
        builder.line(&format!(
            "' SKIPPED: {} returns AzXxx ByVal — VB6 Declare cannot return UDTs.",
            func.c_name
        ));
        builder.line("' Use a C-side shim that writes the result via an out-pointer instead.");
        builder.line(&format!("' Pseudo: m_raw = {}", call));
    } else {
        // Constructor returns Long (a pointer) or void.
        builder.line(&format!("Dim ret_ As Long"));
        builder.line(&format!("ret_ = {}", call));
        builder.line("If ret_ <> 0 Then");
        builder.indent();
        builder.line("CopyMemory m_raw, ByVal ret_, LenB(m_raw)");
        builder.dedent();
        builder.line("End If");
    }
    builder.line("m_owned = True");
    builder.dedent();
    builder.line("End Sub");
    builder.blank();
}

// ============================================================================
// Instance / static methods.
// ============================================================================

fn emit_method(builder: &mut CodeBuilder, raw_record: &str, func: &FunctionDef, ir: &CodegenIR) {
    let method_name = idiomatic_method_name(&func.method_name);
    let visible = visible_user_args(func);
    let args_str = format_arg_list(&visible, ir);
    let is_static = matches!(func.kind, FunctionKind::StaticMethod);
    let takes_self = matches!(
        func.kind,
        FunctionKind::Method | FunctionKind::MethodMut | FunctionKind::DeepCopy
    );

    if !func.doc.is_empty() {
        for d in &func.doc {
            builder.line(&format!("' {}", sanitize_comment(d)));
        }
    }

    // Build call argument list. `self` is passed as VarPtr(m_raw).
    let mut call_args: Vec<String> = Vec::new();
    if takes_self {
        call_args.push("VarPtr(m_raw)".to_string());
    }
    for a in &visible {
        call_args.push(sanitize_identifier(&a.name));
    }
    let call = format!("{}({})", func.c_name, call_args.join(", "));

    let prefix = if is_static { "' Static method." } else { "" };
    if !prefix.is_empty() {
        builder.line(prefix);
    }

    match &func.return_type {
        Some(ret) => {
            let vb_ret = map_type_to_vb6(ret, ir);
            // SKIPPED: returning a UDT ByVal is forbidden in VB6 Declare.
            // We still emit the wrapper so user code compiles, but the
            // actual call may fail — see functions.rs SKIPPED comment.
            if args_str.is_empty() {
                builder.line(&format!(
                    "Public Function {}() As {}",
                    method_name, vb_ret
                ));
            } else {
                builder.line(&format!(
                    "Public Function {}({}) As {}",
                    method_name, args_str, vb_ret
                ));
            }
            builder.indent();
            builder.line(&format!("{} = {}", method_name, call));
            builder.dedent();
            builder.line("End Function");
        }
        None => {
            if args_str.is_empty() {
                builder.line(&format!("Public Sub {}()", method_name));
            } else {
                builder.line(&format!("Public Sub {}({})", method_name, args_str));
            }
            builder.indent();
            builder.line(&call);
            builder.dedent();
            builder.line("End Sub");
        }
    }
    builder.blank();

    // Suppress unused-warning on raw_record (used only inside the impl
    // block above for documentation — kept here for symmetry with the
    // FreeBASIC port).
    let _ = raw_record;
}

// ============================================================================
// Argument helpers
// ============================================================================

fn visible_user_args(func: &FunctionDef) -> Vec<&FunctionArg> {
    let class_lower = func.class_name.to_lowercase();
    func.args
        .iter()
        .filter(|a| a.name != "self" && a.name != class_lower)
        .collect()
}

fn format_arg_list(args: &[&FunctionArg], ir: &CodegenIR) -> String {
    let parts: Vec<String> = args
        .iter()
        .map(|a| {
            let (clause, vb_ty) = match a.ref_kind {
                ArgRefKind::Owned => {
                    let vb = map_type_to_vb6(&a.type_name, ir);
                    let is_udt = ir.find_struct(a.type_name.trim()).is_some()
                        || ir.find_enum(a.type_name.trim()).is_some();
                    if is_udt {
                        ("ByRef", vb)
                    } else {
                        ("ByVal", vb)
                    }
                }
                _ => ("ByVal", "Long".to_string()),
            };
            format!("{} {} As {}", clause, sanitize_identifier(&a.name), vb_ty)
        })
        .collect();
    parts.join(", ")
}
