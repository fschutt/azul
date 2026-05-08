//! Visual Basic 6 binding generator.
//!
//! Targets the *32-bit* Microsoft VB6 toolchain (`VB6.EXE`,
//! `vbc.exe`, the `msvbvm60.dll` runtime that ships with every
//! shipping Windows since Win98). Microsoft discontinued VB6 in 2008
//! but the runtime is still preinstalled on Windows 10/11 — the
//! audience for this binding is essentially zero. The point is to
//! demonstrate that the codegen v2 IR reaches into legacy
//! Windows-only ecosystems with no per-language plumbing changes
//! upstream of `lang_<lang>/`.
//!
//! # 32-bit only
//!
//! There is no 64-bit VB6 compiler. Every variable, every API call,
//! and every pointer in VB6 is 32-bit wide. The generated binding
//! therefore only works against a **32-bit** `azul.dll` (built with
//! the `i686-pc-windows-msvc` Rust target). Linking against the
//! default 64-bit Windows azul build will fail at load time with
//! "Bad DLL Calling Convention" or "File not found" errors.
//!
//! # Output layout
//!
//! VB6 projects are inherently multi-file (one `.bas` BAS module
//! plus one or more `.cls` Class modules plus a `.vbp` Project
//! file). We therefore emit a single concatenated string with file
//! markers:
//!
//! ```text
//! ' ==FILE: Azul.bas ==
//! Attribute VB_Name = "Azul"
//! ...
//! ' ==FILE: App.cls ==
//! VERSION 1.0 CLASS
//! BEGIN
//!   ...
//! ' ==FILE: Window.cls ==
//! ...
//! ' ==FILE: Azul.vbp ==
//! ...
//! ```
//!
//! The orchestrator (or the test harness) splits the string on
//! [`FILE_MARKER`] and writes each chunk to disk. The marker is
//! itself a syntactically valid VB6 line comment (it starts with
//! `'`), so even if a downstream tool fails to split the file the
//! combined text still parses inside the VB6 IDE.
//!
//! # Files emitted
//!
//! 1. **`Azul.bas`** — `Public Type` records, `Public Enum`
//!    declarations, `Public Const` constants, and `Public Declare
//!    Function`/`Sub` extern declarations against `azul.dll`.
//!    Module-level wrapper `Public Function`s for free functions.
//! 2. **`<Type>.cls`** — one VB6 Class Module per disposable type.
//!    `Class_Initialize` constructs the underlying FFI record;
//!    `Class_Terminate` calls the matching `_delete` extern. The
//!    user-facing class names drop the `Az` prefix (`AzApp` →
//!    `App`, `AzWindow` → `Window`).
//! 3. **`Azul.vbp`** — VB6 Project file (`Type=Exe`) declaring every
//!    component VB6 needs to load when opening the project.
//!
//! # Wiring
//!
//! Like the other Wave-2 generators (Haskell/Java/Go/etc.) this
//! module is **not** referenced from `v2/mod.rs`. The orchestrator
//! threads it through after merging the per-language sidecar JSON.
//! Entry point: [`generate`].

use anyhow::Result;

use super::config::CodegenConfig;
use super::generator::CodeBuilder;
use super::ir::CodegenIR;

pub mod functions;
pub mod types;
pub mod vbp;
pub mod wrappers;

/// Library name used in `Declare ... Lib "azul"`. The VB6
/// dynamic loader resolves `azul` to `azul.dll` in the program
/// directory, in `%WINDIR%\System32\`, or anywhere else on `%PATH%`.
///
/// MUST be a 32-bit DLL — VB6 cannot load 64-bit DLLs.
pub const LIB_NAME: &str = "azul";

/// Marker emitted before every file in the multi-file output.
/// The marker starts with `'` so it is a valid VB6 line comment;
/// even when the downstream tool fails to split the multi-file
/// output the combined text still parses inside the VB6 IDE.
///
///     ' ==FILE: Azul.bas ==
///     Attribute VB_Name = "Azul"
///     ...
///
/// Must remain stable; downstream tooling treats it as a contract.
pub const FILE_MARKER: &str = "' ==FILE: ";

/// Closing marker for the file-marker header line.
pub const END_MARKER: &str = " ==";

/// Public entry point. Returns a single string containing every
/// file the binding consists of, separated by [`FILE_MARKER`]
/// header lines. The orchestrator splits on these markers and writes
/// each chunk to its destination path.
pub fn generate(ir: &CodegenIR, config: &CodegenConfig) -> Result<String> {
    let mut out = String::new();

    // 1. Azul.bas — main BAS module: types, enums, declares, free fns.
    push_section(&mut out, "Azul.bas", &generate_bas(ir, config)?);

    // 2. <Type>.cls — one class module per disposable type.
    let class_targets = wrappers::collect_class_targets(ir, config);
    let mut class_names: Vec<String> = Vec::new();
    for s in &class_targets {
        let class_name = wrappers::class_name_for(&s.name);
        let body = wrappers::emit_class_module(s, ir, config)?;
        push_section(&mut out, &format!("{}.cls", class_name), &body);
        class_names.push(class_name);
    }

    // 3. Azul.vbp — project file referencing the BAS + every CLS.
    push_section(&mut out, "Azul.vbp", &vbp::generate_vbp(&class_names));

    Ok(out)
}

/// Build the `Azul.bas` BAS-module body: top banner, `Option Explicit`,
/// `Public Const`s, enums, `Public Type` records, then `Public Declare`
/// externs.
fn generate_bas(ir: &CodegenIR, config: &CodegenConfig) -> Result<String> {
    let mut builder = CodeBuilder::new(&config.indent);

    builder.line("Attribute VB_Name = \"Azul\"");
    emit_bas_header(&mut builder);
    builder.line("Option Explicit");
    builder.blank();

    types::generate_types(&mut builder, ir, config)?;
    builder.blank();

    functions::generate_externals(&mut builder, ir, config)?;
    builder.blank();

    functions::generate_module_wrappers(&mut builder, ir, config)?;

    Ok(builder.finish())
}

fn emit_bas_header(builder: &mut CodeBuilder) {
    builder.line("' ============================================================================");
    builder.line("' Auto-generated Visual Basic 6 bindings for the Azul GUI framework.");
    builder.line("' Generated by azul-doc codegen v2 (lang_vb6).");
    builder.line("' DO NOT EDIT MANUALLY.");
    builder.line("'");
    builder.line("' === 32-BIT ONLY ===");
    builder.line("' VB6 produces and runs 32-bit binaries exclusively. This binding will");
    builder.line("' ONLY work against a 32-bit azul.dll (i686-pc-windows-msvc target).");
    builder.line("' Linking against the default 64-bit Windows azul build fails with");
    builder.line("' 'Bad DLL Calling Convention' or 'File not found' at load time.");
    builder.line("'");
    builder.line("' Runtime: requires msvbvm60.dll (ships with Windows 98 and later).");
    builder.line("'");
    builder.line("' Use:    open Azul.vbp in the VB6 IDE, or compile via vbc.exe.");
    builder.line("' ============================================================================");
}

// ============================================================================
// Multi-file plumbing
// ============================================================================

fn push_section(out: &mut String, path: &str, content: &str) {
    out.push_str(FILE_MARKER);
    out.push_str(path);
    out.push_str(END_MARKER);
    out.push('\n');
    out.push_str(content);
    if !content.ends_with('\n') {
        out.push('\n');
    }
}

// ============================================================================
// Shared type-name helpers (used by submodules)
// ============================================================================

/// The Az-prefixed FFI type name for a given IR type
/// (e.g. `Dom` -> `AzDom`).
pub fn ffi_type_name(name: &str) -> String {
    format!("Az{}", name)
}

/// Map an IR / Rust type name to the corresponding VB6 type token.
///
/// VB6 has no native pointer type, no native unsigned types, and
/// no native 64-bit integer. All pointers collapse to `Long` (the
/// VB6-idiomatic "Long-as-pointer" convention). Unsigned types
/// widen to the next signed type. 64-bit integer types map to
/// `Currency` (which is 8 bytes on the stack so the calling
/// convention lines up — but math operations are scaled by 10,000
/// because Currency is a 4-decimal fixed-point type internally).
pub fn map_type_to_vb6(rust_type: &str, ir: &CodegenIR) -> String {
    let trimmed = rust_type.trim();

    // Pointers / references all collapse to Long.
    if trimmed.starts_with("*const ")
        || trimmed.starts_with("*mut ")
        || trimmed.starts_with("&mut ")
        || trimmed.starts_with('&')
    {
        // SKIPPED inner type: VB6 has no typed pointers. Long-as-pointer
        // is the convention; the user must call CopyMemory / extract via
        // RtlMoveMemory if they need to dereference.
        return "Long".to_string();
    }

    match trimmed {
        // Void / unit — caller treats as Sub (no return) at call sites.
        "void" | "c_void" | "()" => String::new(),

        // C bool is 1 byte, VB6 Boolean is 2 bytes (VARIANT_BOOL). Use
        // Byte to match the C ABI exactly. 0 = false, non-zero = true.
        "bool" | "GLboolean" => "Byte".to_string(),

        // Signed/unsigned 8-bit.
        "i8" | "c_char" | "char" | "u8" | "c_uchar" => "Byte".to_string(),

        // 16-bit. VB6 Integer is 16-bit (NOT 32-bit like in .NET).
        "i16" | "u16" => "Integer".to_string(),

        // 32-bit. VB6 Long is 32-bit signed. Unsigned 32-bit silently
        // widens to a 32-bit signed Long — VB6 has no native u32.
        "i32"
        | "c_int"
        | "u32"
        | "c_uint"
        | "GLint"
        | "GLuint"
        | "GLenum"
        | "GLsizei"
        | "GLbitfield" => "Long".to_string(),

        // f32 -> Single, f64 -> Double.
        "f32" | "GLfloat" | "GLclampf" => "Single".to_string(),
        "f64" | "GLdouble" | "GLclampd" => "Double".to_string(),

        // 64-bit integer. VB6 has no native 64-bit int; Currency is
        // 8 bytes on the stack so the calling convention matches a
        // C int64_t/uint64_t argument by value, but Currency math
        // scales values by 10000. Users must divide by 10000 manually
        // before doing arithmetic.
        // SKIPPED: i64/u64 are not natively supported by VB6 — using
        //          Currency type with manual scaling. See top-of-file
        //          warning block.
        "i64" | "u64" | "GLint64" | "GLuint64" => "Currency".to_string(),

        // VB6 is 32-bit only, so usize/isize are always 32 bits.
        "usize" | "size_t" | "uintptr_t" | "isize" | "ssize_t" | "intptr_t" | "GLsizeiptr"
        | "GLintptr" => "Long".to_string(),

        _ => {
            if ir.find_struct(trimmed).is_some()
                || ir.find_enum(trimmed).is_some()
                || ir.find_type_alias(trimmed).is_some()
                || ir.callback_typedefs.iter().any(|c| c.name == trimmed)
            {
                ffi_type_name(trimmed)
            } else {
                // Unknown — opaque pointer.
                "Long".to_string()
            }
        }
    }
}

/// VB6 reserved-word check. VB6 is case-insensitive for identifiers,
/// so we lowercase the candidate before comparing. Reserved-word
/// collisions are mangled with a trailing underscore by
/// [`sanitize_identifier`].
pub fn is_vb6_reserved(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "as"
            | "binary"
            | "boolean"
            | "byref"
            | "byte"
            | "byval"
            | "call"
            | "case"
            | "circle"
            | "class"
            | "class_initialize"
            | "class_terminate"
            | "const"
            | "currency"
            | "date"
            | "debug"
            | "declare"
            | "dim"
            | "do"
            | "double"
            | "each"
            | "else"
            | "elseif"
            | "empty"
            | "end"
            | "endif"
            | "enum"
            | "eqv"
            | "event"
            | "exit"
            | "explicit"
            | "false"
            | "for"
            | "friend"
            | "function"
            | "get"
            | "global"
            | "gosub"
            | "goto"
            | "if"
            | "imp"
            | "in"
            | "input"
            | "integer"
            | "is"
            | "lib"
            | "like"
            | "line"
            | "load"
            | "local"
            | "lock"
            | "long"
            | "loop"
            | "lset"
            | "me"
            | "mid"
            | "mod"
            | "new"
            | "next"
            | "not"
            | "nothing"
            | "null"
            | "object"
            | "on"
            | "open"
            | "option"
            | "optional"
            | "or"
            | "paramarray"
            | "preserve"
            | "print"
            | "private"
            | "property"
            | "psaet"
            | "public"
            | "raiseevent"
            | "redim"
            | "rem"
            | "resume"
            | "return"
            | "rset"
            | "scale"
            | "seek"
            | "select"
            | "set"
            | "shared"
            | "single"
            | "static"
            | "step"
            | "stop"
            | "string"
            | "sub"
            | "then"
            | "time"
            | "to"
            | "true"
            | "type"
            | "typeof"
            | "unload"
            | "until"
            | "variant"
            | "wend"
            | "while"
            | "with"
            | "withevents"
            | "xor"
    )
}

/// Sanitize an identifier for use as a VB6 field / argument /
/// constant / variable name. Reserved-word collisions get a
/// trailing underscore. VB6 has no verbatim-identifier syntax.
pub fn sanitize_identifier(name: &str) -> String {
    if is_vb6_reserved(name) {
        format!("{}_", name)
    } else {
        name.to_string()
    }
}

/// Convert a snake_case or camelCase name to PascalCase (idiomatic
/// for VB6 function / property / class names).
pub fn to_pascal_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut upper_next = true;
    for c in s.chars() {
        if c == '_' {
            upper_next = true;
        } else if upper_next {
            out.extend(c.to_uppercase());
            upper_next = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// Sanitize a doc-comment line so it can sit safely inside a VB6
/// `'` line comment. We collapse newlines into spaces — `'` only
/// terminates at end-of-line.
pub fn sanitize_comment(s: &str) -> String {
    s.replace('\n', " ").replace('\r', " ")
}

/// Idiomatic VB6 method-name conversion. VB6 has many reserved
/// words; conflicts are mangled by [`sanitize_identifier`].
pub fn idiomatic_method_name(method_name: &str) -> String {
    if method_name == "new" {
        // `New` is reserved; `Create` is the VB6-idiomatic constructor name.
        return "Create".to_string();
    }
    if method_name.contains('_') {
        return sanitize_identifier(&to_pascal_case(method_name));
    }
    let mut chars = method_name.chars();
    let first = match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>(),
        None => return String::new(),
    };
    sanitize_identifier(&(first + chars.as_str()))
}
