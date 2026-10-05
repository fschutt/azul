//! VB6 Class Module (`.cls`) emission.
//!
//! For every IR struct that has a matching `<TypeName>_delete` C
//! function, we emit a VB6 Class Module (`.cls` file). The class:
//!
//! 1. Holds the underlying FFI record (`AzTypeName`) by value in a private `m_raw` field, plus an
//!    `m_owned` flag so wrap-existing factories can opt out of automatic deletion.
//! 2. Exposes one or more `Public Sub Init...(...)` initialisers per IR `FunctionKind::Constructor`
//!    / `FunctionKind::Default` method on the type. VB6 does **not** support overloaded
//!    constructors, so each constructor gets a distinct `Init<Suffix>` name.
//! 3. Implements `Class_Initialize` / `Class_Terminate` — the latter calls the matching `_delete`
//!    extern when `m_owned` is True.
//! 4. Surfaces every non-trait method as a `Public Function`/`Sub` delegating to the FFI symbol
//!    with `m_raw` passed as the self-pointer.
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

use std::collections::BTreeSet;

use anyhow::Result;

use super::{
    super::{
        config::CodegenConfig,
        generator::CodeBuilder,
        ir::{
            ArgRefKind, CodegenIR, FunctionArg, FunctionDef, FunctionKind, StructDef, TypeCategory,
        },
    },
    functions::{arg_clause_and_type, call_lines, uses_byref_twin},
    ffi_type_name, idiomatic_method_name, map_type_to_vb6, sanitize_comment, sanitize_identifier,
};

// ============================================================================
// Discovery
// ============================================================================

/// Collect every struct that should be wrapped in a `.cls` Class
/// Module — i.e. every struct with a matching `_delete` extern.
pub fn collect_class_targets<'a>(ir: &'a CodegenIR, config: &CodegenConfig) -> Vec<&'a StructDef> {
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
pub fn emit_class_module(s: &StructDef, ir: &CodegenIR, config: &CodegenConfig) -> Result<String> {
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
    builder.line(&"Public Sub WrapRaw(ByVal rawPtr As Long)".to_string());
    builder.indent();
    builder.line(&"CopyMemory m_raw, ByVal rawPtr, LenB(m_raw)".to_string());
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

    if returns_self {
        // The record comes back by value: through the Byref twin's
        // out-pointer, straight into m_raw (functions::call_lines).
        for l in call_lines(func, ir, &call_args, Some("m_raw")) {
            builder.line(&l);
        }
    } else if func.return_type.is_some() && !uses_byref_twin(func, ir) {
        // Constructor returns a pointer to the record.
        builder.line(&"Dim ret_ As Long".to_string());
        for l in call_lines(func, ir, &call_args, Some("ret_")) {
            builder.line(&l);
        }
        builder.line("If ret_ <> 0 Then");
        builder.indent();
        builder.line("CopyMemory m_raw, ByVal ret_, LenB(m_raw)");
        builder.dedent();
        builder.line("End If");
    } else {
        // Anything else (an Option/Result of the record, or nothing):
        // the class holds only the record itself, so the result is
        // dropped here - call the Declare directly to keep it.
        if let Some(ret) = func.return_type.as_deref() {
            builder.line(&format!("Dim ret_ As {}", map_type_to_vb6(ret, ir)));
        }
        for l in call_lines(func, ir, &call_args, Some("ret_")) {
            builder.line(&l);
        }
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

    // Build call argument list, one per `func.args` entry. A borrowed
    // `self` is a pointer (VarPtr(m_raw)); a consumed `self` (a builder
    // method taking the record by value) goes to the Byref twin as the
    // record itself, which the call then owns.
    let receiver = func.args.iter().find(|a| func.is_receiver_arg(a));
    let consumes_self = takes_self && receiver.is_some_and(|r| r.ref_kind == ArgRefKind::Owned);
    let mut call_args: Vec<String> = Vec::new();
    if takes_self && receiver.is_some() {
        let this = if consumes_self { "m_raw" } else { "VarPtr(m_raw)" };
        call_args.push(this.to_string());
    }
    for a in &visible {
        call_args.push(sanitize_identifier(&a.name));
    }

    let prefix = if is_static { "' Static method." } else { "" };
    if !prefix.is_empty() {
        builder.line(prefix);
    }

    match &func.return_type {
        Some(ret) => {
            // A VB6 Function may return a UDT (only a Declare cannot); the
            // result lands in a local, which the Byref twin fills.
            let vb_ret = map_type_to_vb6(ret, ir);
            if args_str.is_empty() {
                builder.line(&format!("Public Function {}() As {}", method_name, vb_ret));
            } else {
                builder.line(&format!(
                    "Public Function {}({}) As {}",
                    method_name, args_str, vb_ret
                ));
            }
            builder.indent();
            builder.line(&format!("Dim r_ As {}", vb_ret));
            for l in call_lines(func, ir, &call_args, Some("r_")) {
                builder.line(&l);
            }
            if consumes_self {
                builder.line("m_owned = False ' the call took the record");
            }
            builder.line(&format!("{} = r_", method_name));
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
            for l in call_lines(func, ir, &call_args, None) {
                builder.line(&l);
            }
            if consumes_self {
                builder.line("m_owned = False ' the call took the record");
            }
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
    func.args.iter().filter(|a| !func.is_receiver_arg(a)).collect()
}

fn format_arg_list(args: &[&FunctionArg], ir: &CodegenIR) -> String {
    let parts: Vec<String> = args
        .iter()
        .map(|a| {
            let (clause, vb_ty) = arg_clause_and_type(&a.ref_kind, &a.type_name, ir);
            format!("{} {} As {}", clause, sanitize_identifier(&a.name), vb_ty)
        })
        .collect();
    parts.join(", ")
}

#[cfg(test)]
mod field_access_tests {
    use std::sync::OnceLock;

    use super::super::super::config::CodegenConfig;

    fn generated() -> &'static str {
        static OUT: OnceLock<String> = OnceLock::new();
        OUT.get_or_init(|| {
            let ir = crate::codegen::v2::bug_classes::ir();
            super::super::generate(ir, &CodegenConfig::c_header()).expect("vb6 codegen")
        })
    }

    /// The text of one emitted file.
    fn file(name: &str) -> &'static str {
        let src = generated();
        let marker = format!("{}{}{}\n", super::super::FILE_MARKER, name, super::super::END_MARKER);
        let start = src.find(&marker).unwrap_or_else(|| panic!("no file {}", name)) + marker.len();
        let end = src[start..]
            .find(super::super::FILE_MARKER)
            .map_or(src.len(), |e| start + e);
        &src[start..end]
    }

    /// One member, from its header to the next `End <kind>`.
    fn member<'a>(src: &'a str, header: &str, kind: &str) -> &'a str {
        let start = src.find(header).unwrap_or_else(|| panic!("no `{}`", header));
        let end = src[start..].find(&format!("End {}", kind)).expect("end of member");
        &src[start..start + end]
    }

    #[test]
    fn no_class_module_is_named_after_a_keyword_or_a_vb6_global_object() {
        let src = generated();
        assert!(!src.contains("' ==FILE: String.cls =="), "String is a VB6 keyword");
        assert!(!src.contains("' ==FILE: App.cls =="), "App shadows VB6's global App object");
        assert!(file("AzulString.cls").contains("Attribute VB_Name = \"AzulString\""));
        assert!(file("AzulApp.cls").contains("Attribute VB_Name = \"AzulApp\""));
        assert!(file("Azul.vbp").contains("Class=AzulApp; AzulApp.cls"));
    }

    #[test]
    fn the_window_title_is_a_string_property_that_releases_the_old_value() {
        let cls = file("FullWindowState.cls");
        let get = member(cls, "Public Property Get Title() As String", "Property");
        assert!(!get.contains("_delete"), "reading must not free the field:\n{}", get);
        let set = member(cls, "Public Property Let Title(ByVal v As String)", "Property");
        assert!(set.contains("AzString_delete VarPtr(m_raw.title)"), "{}", set);
        assert!(set.contains("m_raw.title = nv"), "{}", set);
    }

    #[test]
    fn the_window_state_property_deep_copies_and_consumes() {
        let cls = file("WindowCreateOptions.cls");
        let get = member(cls, "Public Property Get WindowState() As FullWindowState", "Property");
        assert!(get.contains("VarPtr(m_raw.window_state)"), "{}", get);
        assert!(get.contains("AzFullWindowState_clone"), "a deep copy:\n{}", get);
        let set = member(cls, "Public Property Set WindowState(ByVal v As FullWindowState)", "Property");
        assert!(set.contains("v.MoveRawInto VarPtr(nv)"), "the argument is consumed:\n{}", set);
        assert!(set.contains("AzFullWindowState_delete VarPtr(m_raw.window_state)"), "{}", set);
        let mv = member(file("FullWindowState.cls"), "Friend Sub MoveRawInto(ByVal dst As Long)", "Sub");
        assert!(mv.contains("m_owned = False"), "{}", mv);
    }

    #[test]
    fn a_plain_record_field_is_a_friend_property_copied_in_and_out() {
        let cls = file("FullWindowState.cls");
        let set = member(cls, "Friend Property Let Size(ByRef v As AzWindowSize)", "Property");
        assert!(set.contains("m_raw.size = v"), "{}", set);
        assert!(cls.contains("Friend Property Get Size() As AzWindowSize"));
    }

    #[test]
    fn a_text_input_text_field_is_writable_even_though_get_text_is_a_method() {
        assert!(file("TextInputState.cls").contains("Public Property Set Text(ByVal v As U32Vec)"));
    }

    #[test]
    fn clone_takes_no_extra_argument() {
        assert!(file("TextInputState.cls").contains("Public Function Clone() As AzTextInputState"));
    }
}
