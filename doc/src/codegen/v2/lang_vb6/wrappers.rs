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
//! 5. One property per public by-value field (see [`emit_field_properties`]): getters return
//!    independent values, setters release the old value and move the new one in; a class-typed
//!    argument is consumed through `Friend Sub MoveRawInto`.
//!
//! User-facing class names drop the `Az` prefix:  `AzWindow` → `Window`;
//! a name VB6 already owns (`String`, the global `App` object) becomes
//! `AzulString` / `AzulApp`. Each lives in its own `.cls` file.
//!
//! Open question (not fixed here): VB6 `Declare` is stdcall, libazul's
//! 32-bit exports are cdecl - see the note in [`super::functions`].
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
        field_access_classic::{self as fa, FieldKind},
        generator::CodeBuilder,
        ir::{
            ArgRefKind, CodegenIR, FunctionArg, FunctionDef, FunctionKind, StructDef, TypeCategory,
        },
    },
    functions::{arg_clause_and_type, call_lines, uses_byref_twin},
    ffi_type_name, idiomatic_method_name, is_vb6_reserved, map_type_to_vb6, sanitize_comment,
    sanitize_identifier, to_pascal_case,
};

/// `Azul.bas` helper: `AzString` -> VB6 `String` (UTF-8 decoded), NOT
/// consuming its argument.
const STRING_READ_HELPER: &str = "AzulStringRead";
/// `Azul.bas` helper: VB6 `String` -> fresh `AzString` (UTF-8 encoded).
const STRING_NEW_HELPER: &str = "AzulStringNew";

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
/// `Window` -> `Window`. A name VB6 already owns - a keyword (`String`) or
/// one of the runtime's global objects (`App`, `Screen`, ...), which a
/// class of that name would shadow project-wide - becomes `Azul<Name>`
/// (`AzulString`, `AzulApp`).
pub fn class_name_for(raw: &str) -> String {
    let base = raw.strip_prefix("Az").unwrap_or(raw);
    if is_vb6_reserved(base) || is_vb6_global_object(base) {
        format!("Azul{}", base)
    } else {
        base.to_string()
    }
}

/// VB6's predeclared global objects and runtime libraries: a class module
/// with one of these names hides the built-in everywhere in the project.
fn is_vb6_global_object(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    matches!( // allow-api-name: VB6 runtime globals, not API names.
        lower.as_str(),
        "app"
            | "screen"
            | "printer"
            | "printers"
            | "clipboard"
            | "forms"
            | "err"
            | "debug"
            | "licenses"
            | "global"
            | "vb"
            | "vba"
            | "collection"
            | "form"
            | "control"
            | "object"
    )
}

/// The kernel32 `Declare`s [`emit_string_helpers`] needs. VB6 only accepts
/// a `Declare` in a module's declarations section (before the first
/// procedure), hence a separate emitter.
pub fn emit_string_helper_declares(builder: &mut CodeBuilder) {
    builder.line("' UTF-8 conversions for the string-field helpers (AzulStringRead / New).");
    builder.line("Private Declare Function MultiByteToWideChar Lib \"kernel32\" (ByVal CodePage As Long, ByVal dwFlags As Long, ByVal lpMultiByteStr As Long, ByVal cbMultiByte As Long, ByVal lpWideCharStr As Long, ByVal cchWideChar As Long) As Long");
    builder.line("Private Declare Function WideCharToMultiByte Lib \"kernel32\" (ByVal CodePage As Long, ByVal dwFlags As Long, ByVal lpWideCharStr As Long, ByVal cchWideChar As Long, ByVal lpMultiByteStr As Long, ByVal cbMultiByte As Long, ByVal lpDefaultChar As Long, ByVal lpUsedDefaultChar As Long) As Long");
    builder.blank();
}

/// `AzulStringRead` / `AzulStringNew` for `Azul.bas` (UTF-8 through
/// kernel32's code-page conversions). Emits nothing when the IR lacks the
/// string class's byte layout or its copying constructor.
pub fn emit_string_helpers(builder: &mut CodeBuilder, ir: &CodegenIR) {
    let (Some(st), Some(copy), Some((vec, ptr, len))) =
        (fa::string_class(ir), fa::string_copy_fn(ir), fa::string_layout(ir))
    else {
        return;
    };
    let az = ffi_type_name(&st.name);
    let (vec, ptr, len) = (sanitize_identifier(&vec), sanitize_identifier(&ptr), sanitize_identifier(&len));
    builder.line("' --------------------------------------------------------------------");
    builder.line("' String fields: UTF-8 <-> VB6 String. AzulStringRead never frees.");
    builder.line("' --------------------------------------------------------------------");
    builder.line(&format!("Public Function {}(ByRef s As {}) As String", STRING_READ_HELPER, az));
    builder.indent();
    builder.line("Dim n As Long, w As Long, r As String");
    builder.line(&format!("n = s.{}.{}", vec, len));
    builder.line("If n <= 0 Then Exit Function");
    builder.line(&format!("w = MultiByteToWideChar(65001, 0, s.{}.{}, n, 0, 0)", vec, ptr));
    builder.line("r = String$(w, vbNullChar)");
    builder.line(&format!("MultiByteToWideChar 65001, 0, s.{}.{}, n, StrPtr(r), w", vec, ptr));
    builder.line(&format!("{} = r", STRING_READ_HELPER));
    builder.dedent();
    builder.line("End Function");
    builder.blank();
    builder.line(&format!("Public Function {}(ByVal v As String) As {}", STRING_NEW_HELPER, az));
    builder.indent();
    builder.line(&format!("Dim n As Long, buf() As Byte, r As {}", az));
    builder.line("n = WideCharToMultiByte(65001, 0, StrPtr(v), Len(v), 0, 0, 0, 0)");
    builder.line("If n > 0 Then");
    builder.indent();
    builder.line("ReDim buf(0 To n - 1)");
    builder.line("WideCharToMultiByte 65001, 0, StrPtr(v), Len(v), VarPtr(buf(0)), n, 0, 0");
    let some: Vec<String> = ["VarPtr(buf(0))", "0", "n"].iter().map(|a| a.to_string()).collect();
    for l in call_lines(copy, ir, &some, Some("r")) {
        builder.line(&l);
    }
    builder.dedent();
    builder.line("Else");
    builder.indent();
    let none: Vec<String> = ["0", "0", "0"].iter().map(|a| a.to_string()).collect();
    for l in call_lines(copy, ir, &none, Some("r")) {
        builder.line(&l);
    }
    builder.dedent();
    builder.line("End If");
    builder.line(&format!("{} = r", STRING_NEW_HELPER));
    builder.dedent();
    builder.line("End Function");
    builder.blank();
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

    emit_move_raw_into(&mut builder, s, ir);
    emit_field_properties(&mut builder, s, ir, config);

    Ok(builder.finish())
}

/// `MoveRawInto`: hands the record to a consuming setter of another class
/// (moved when owned - this object stops owning it - else deep-copied).
/// `Friend`, so it never shows on the public surface.
fn emit_move_raw_into(builder: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR) {
    builder.line("' MoveRawInto: write the record to dst; an owned record MOVES (this object");
    builder.line("' no longer frees it), a borrowed one is deep-copied.");
    builder.line("Friend Sub MoveRawInto(ByVal dst As Long)");
    builder.indent();
    builder.line("If m_owned Then");
    builder.indent();
    builder.line("CopyMemory ByVal dst, m_raw, LenB(m_raw)");
    builder.line("m_owned = False");
    builder.dedent();
    builder.line("Else");
    builder.indent();
    match fa::clone_fn(ir, &s.name) {
        Some(clone) => {
            builder.line(&format!("Dim c_ As {}", ffi_type_name(&s.name)));
            for l in call_lines(clone, ir, &["VarPtr(m_raw)".to_string()], Some("c_")) {
                builder.line(&l);
            }
            builder.line("CopyMemory ByVal dst, c_, LenB(c_)");
        }
        None => builder.line(&format!(
            "Err.Raise 5, \"{}\", \"a borrowed record without a deep copy cannot be moved\"",
            class_name_for(&s.name)
        )),
    }
    builder.dedent();
    builder.line("End If");
    builder.dedent();
    builder.line("End Sub");
    builder.blank();
}

/// One property per public by-value field (see the module docs).
///
/// - scalars, `Boolean` and `String` (decoded without consuming the field):
///   `Public Property Get` / `Let`;
/// - a field whose type has a class: `Public Property Get` (a new object
///   holding a deep copy) / `Set` (consumes the argument via `MoveRawInto`);
/// - unit enums and plain records: `Friend` (VB6 forbids standard-module
///   types on a class's public surface), `Get` a copy / `Let` by reference.
///
/// Every setter releases the old value first. An api.json method of the
/// same name keeps its name; the property becomes `<Field>Field`. Nested
/// fields are read-modify-write:
///
/// ```text
/// Dim ws As FullWindowState
/// Set ws = opts.WindowState        ' deep copy
/// ws.Title = "Hello"               ' releases the old title
/// Dim sz As AzWindowSize
/// sz = ws.Size: sz.dimensions.width = 800: ws.Size = sz
/// Set opts.WindowState = ws        ' consumes ws
/// ```
fn emit_field_properties(builder: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR, config: &CodegenConfig) {
    let classes: BTreeSet<String> = collect_class_targets(ir, config)
        .iter()
        .map(|c| c.name.clone())
        .collect();
    let mut taken: BTreeSet<String> = [
        "m_raw",
        "m_owned",
        "wrapraw",
        "getrawptr",
        "moverawinto",
        "class_initialize",
        "class_terminate",
        "init",
    ]
    .iter()
    .map(|n| n.to_string())
    .collect();
    for func in ir.functions_for_class(&s.name) {
        taken.insert(idiomatic_method_name(&func.method_name).to_ascii_lowercase());
        taken.insert(format!("init{}", idiomatic_method_name(&func.method_name)).to_ascii_lowercase());
    }
    for a in fa::accessible_fields(s, ir, config) {
        let comp = format!("m_raw.{}", sanitize_identifier(&a.field.name));
        let base = sanitize_identifier(&to_pascal_case(a.field.name.trim_start_matches('_')));
        let name = if taken.contains(&base.to_ascii_lowercase()) {
            format!("{}Field", base)
        } else {
            base
        };
        if !taken.insert(name.to_ascii_lowercase()) {
            continue;
        }
        let release = |builder: &mut CodeBuilder| {
            if let Some(d) = fa::delete_fn(ir, a.ty) {
                for l in call_lines(d, ir, &[format!("VarPtr({})", comp)], None) {
                    builder.line(&l);
                }
            }
        };
        let clone_into = |builder: &mut CodeBuilder, var: &str| -> bool {
            match fa::clone_fn(ir, a.ty) {
                Some(c) => {
                    for l in call_lines(c, ir, &[format!("VarPtr({})", comp)], Some(var)) {
                        builder.line(&l);
                    }
                    true
                }
                None => false,
            }
        };
        match a.kind {
            FieldKind::Prim { is_bool: true } => {
                builder.line(&format!("Public Property Get {}() As Boolean", name));
                builder.line(&format!("    {} = ({} <> 0)", name, comp));
                builder.line("End Property");
                builder.line(&format!("Public Property Let {}(ByVal v As Boolean)", name));
                builder.line(&format!("    If v Then {} = 1 Else {} = 0", comp, comp));
                builder.line("End Property");
            }
            FieldKind::Prim { .. } | FieldKind::UnitEnum => {
                let vis = if matches!(a.kind, FieldKind::UnitEnum) { "Friend" } else { "Public" };
                let ty = map_type_to_vb6(a.ty, ir);
                if ty.is_empty() {
                    continue;
                }
                builder.line(&format!("{} Property Get {}() As {}", vis, name, ty));
                builder.line(&format!("    {} = {}", name, comp));
                builder.line("End Property");
                builder.line(&format!("{} Property Let {}(ByVal v As {})", vis, name, ty));
                builder.line(&format!("    {} = v", comp));
                builder.line("End Property");
            }
            FieldKind::Str { .. } => {
                if fa::string_copy_fn(ir).is_none() || fa::string_layout(ir).is_none() {
                    continue;
                }
                builder.line(&format!("Public Property Get {}() As String", name));
                builder.line(&format!("    {} = {}({})", name, STRING_READ_HELPER, comp));
                builder.line("End Property");
                builder.line(&format!("Public Property Let {}(ByVal v As String)", name));
                builder.indent();
                builder.line(&format!("Dim nv As {}", ffi_type_name(a.ty)));
                builder.line(&format!("nv = {}(v)", STRING_NEW_HELPER));
                release(builder);
                builder.line(&format!("{} = nv", comp));
                builder.dedent();
                builder.line("End Property");
            }
            FieldKind::Value { .. } if classes.contains(a.ty) => {
                let cls = class_name_for(a.ty);
                let raw = ffi_type_name(a.ty);
                if fa::clone_fn(ir, a.ty).is_some() {
                    builder.line(&format!("Public Property Get {}() As {}", name, cls));
                    builder.indent();
                    builder.line(&format!("Dim r_ As {}", raw));
                    clone_into(builder, "r_");
                    builder.line(&format!("Dim o_ As {}", cls));
                    builder.line(&format!("Set o_ = New {}", cls));
                    builder.line("o_.WrapRaw VarPtr(r_)");
                    builder.line(&format!("Set {} = o_", name));
                    builder.dedent();
                    builder.line("End Property");
                }
                builder.line(&format!("Public Property Set {}(ByVal v As {})", name, cls));
                builder.indent();
                builder.line(&format!("Dim nv As {}", raw));
                builder.line("v.MoveRawInto VarPtr(nv)");
                release(builder);
                builder.line(&format!("{} = nv", comp));
                builder.dedent();
                builder.line("End Property");
            }
            FieldKind::Value { delete, clone } => {
                let raw = map_type_to_vb6(a.ty, ir);
                if raw.is_empty() || raw == "Long" {
                    continue;
                }
                if delete.is_none() || clone.is_some() {
                    builder.line(&format!("Friend Property Get {}() As {}", name, raw));
                    builder.indent();
                    if delete.is_some() {
                        builder.line(&format!("Dim r_ As {}", raw));
                        clone_into(builder, "r_");
                        builder.line(&format!("{} = r_", name));
                    } else {
                        builder.line(&format!("{} = {}", name, comp));
                    }
                    builder.dedent();
                    builder.line("End Property");
                }
                // The field takes over the record's heap memory.
                builder.line(&format!("Friend Property Let {}(ByRef v As {})", name, raw));
                builder.indent();
                release(builder);
                builder.line(&format!("{} = v", comp));
                builder.dedent();
                builder.line("End Property");
            }
        }
        builder.blank();
    }
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
    // The receiver is the first argument of every self-taking kind,
    // whatever api.json named it (`instance` for the deep copy).
    let receiver = if takes_self { func.args.first() } else { None };
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
    let takes_self = matches!(
        func.kind,
        FunctionKind::Method | FunctionKind::MethodMut | FunctionKind::DeepCopy
    );
    if takes_self {
        func.args.iter().skip(1).collect()
    } else {
        func.args.iter().filter(|a| !func.is_receiver_arg(a)).collect()
    }
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
