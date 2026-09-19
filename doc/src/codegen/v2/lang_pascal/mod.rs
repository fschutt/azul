//! Pascal (Free Pascal / Lazarus) binding generator.
//!
//! Generates a single `azul.pas` unit file that:
//!
//! 1. Declares all C-ABI types as Pascal `record` definitions (POD/blittable), Pascal enums for
//!    unit-only enumerations, and variant records for tagged unions.
//! 2. Forward-declares typed pointers (`PAzFoo = ^TAzFoo;`) at the top of the type block so all
//!    later `record` and `external` declarations may refer to them in any order.
//! 3. Declares every C-API function as a `cdecl; external 'azul';` import using the verbatim `Az`
//!    prefix so the linker can match symbol names.
//! 4. Wraps every type that owns heap memory (i.e. has a matching `<TypeName>_delete` C function)
//!    in an idiomatic Pascal `class` whose `destructor Destroy; override;` calls the matching
//!    `_delete` (see `wrappers.rs` for the fluent / consume rules and the string overloads).
//! 5. Emits the host-invoker callback surface, the generic `TAz<App><T>` helper and the
//!    `az<Variant>` enum aliases (see `managed.rs`).
//!
//! # Unit directives
//!
//! The unit is compiled in `{$mode delphi}{$H+}` (it uses Delphi-syntax generics; the user's
//! program picks its own mode — an objfpc program writes `specialize TAzApp<TMyModel>`), with
//! `{$PACKRECORDS C}` for C struct layout AND `{$PACKENUM 4}` because Delphi mode defaults enums to
//! ONE byte while every `#[repr(C)]` enum in libazul is a C `int` (4 bytes). Without `PACKENUM 4`
//! every enum-bearing record is mis-laid-out and the first layout callback dies with an EBusError.
//! The `initialization` block re-checks the sizes at load time.
//!
//! # Output structure (high-level)
//!
//! ```pascal
//! unit Azul;
//! (mode delphi, H+, PACKRECORDS C, PACKENUM 4, linklib azul)
//!
//! interface
//! uses ctypes, Math, SysUtils;
//!
//! const
//!   AzulLib = 'azul';
//!
//! type
//!   { Forward pointer declarations }
//!   PAzApp = ^TAzApp;
//!   { POD records }        TAzAppConfig = record ... end;
//!   { Unit enums }         TAzButtonType = (TAzButtonType_Primary, ...);
//!   { Variant records }    TAzOptionI64 = record case Tag: cuint8 of ... end;
//!
//! { External declarations }
//! function AzApp_create(data: TAzRefAny; config: TAzAppConfig): TAzApp; cdecl; external AzulLib;
//!
//! { Host-invoker plumbing, then ONE type block with: wrapper forward decls, the callback
//!   types (TAz<K>Event / Proc / Func<T> / ModelFunc<T> ...), the wrapper classes
//!   (TDom, TButton, ...), the options view and TAzApp<T>. Then azul_register_*,
//!   string helpers and the az<Variant> constants. }
//!
//! implementation
//! { handle table, releaser, invoker stubs, method bodies, initialization self-check }
//! end.
//! ```

use anyhow::Result;

use super::{config::CodegenConfig, generator::CodeBuilder, ir::CodegenIR};

pub mod functions;
pub mod lpi;
pub mod managed;
pub mod types;
pub mod wrappers;

/// Library name used in `external 'azul';`.
///
/// Matches the prebuilt artifact's name without extension; FPC resolves
/// this to `azul.dll` on Windows, `libazul.so` on Linux, and
/// `libazul.dylib` on macOS.
pub const LIB_NAME: &str = "azul";

/// Public entry point. Produces the full `azul.pas` unit as a `String`.
///
/// The caller is expected to write the result to disk (e.g.
/// `target/codegen/v2/azul.pas`) and to separately emit the accompanying
/// Lazarus `.lpi` project via [`lpi::generate_lpi`].
pub fn generate(ir: &CodegenIR, config: &CodegenConfig) -> Result<String> {
    let mut builder = CodeBuilder::new("  ");

    // File banner
    emit_header(&mut builder);

    // Unit declaration + compiler directives
    builder.line("unit Azul;");
    builder.blank();
    builder.line("{$mode delphi}{$H+}");
    // C struct layout for every record ...
    builder.line("{$PACKRECORDS C}");
    // ... and 4-byte enums: Delphi mode defaults to {$Z1} (1-byte enums),
    // libazul's repr(C) enums are C ints. Both directives are unconditional
    // and re-verified at unit load (see managed::emit_managed_initialization).
    builder.line("{$PACKENUM 4}");
    builder.line("{$MACRO ON}");
    // Function references / anonymous methods exist on FPC 3.3.1+ (and
    // Delphi) only; the `reference to` callback overloads are guarded by the
    // same test everywhere in the unit.
    builder.line(managed::FUNCREF_GUARD);
    builder.line("{$modeswitch functionreferences}");
    builder.line("{$modeswitch anonymousfunctions}");
    builder.line(managed::FUNCREF_GUARD_END);
    // Auto-link the native library so users don't need `-k-lazul` on the fpc
    // command line — only the library search path (`-Fl.` / `-k-L.`) or a
    // system-installed libazul is still required. `azul` resolves to
    // libazul.so / libazul.dylib / azul.dll per platform.
    builder.line(&format!("{{$linklib {}}}", LIB_NAME));
    builder.blank();

    // === interface section ===
    builder.line("interface");
    builder.blank();
    // `Math` for SetExceptionMask in the initialization block (see
    // managed::emit_managed_initialization for why the unit must mask the
    // FPU); `SysUtils` for EAzulError.
    builder.line("uses ctypes, Math, SysUtils;");
    builder.blank();

    // Library name constant
    builder.line("const");
    builder.indent();
    builder.line(&format!("AzulLib = '{}';", LIB_NAME));
    builder.dedent();
    builder.blank();

    let targets = wrappers::wrapper_target_names(ir, config);

    // 1. Type forward declarations + record/enum/variant-record definitions
    types::generate_types(&mut builder, ir, config)?;

    // 2. External cdecl function declarations
    functions::generate_externals(&mut builder, ir, config)?;

    // 3. Host-invoker plumbing (invoker types, externals, base classes,
    //    azul_refany_create / get).
    managed::emit_managed_interface(&mut builder, ir, &targets);

    // 4. One type block: wrapper forward decls, callback surface, wrapper
    //    classes, options view + TAz<App><T>.
    wrappers::generate_wrapper_interface(&mut builder, ir, config)?;

    // 5. azul_register_<kind>, string helpers, az<Variant> aliases.
    managed::emit_managed_interface_tail(&mut builder, ir, config);

    // === implementation section ===
    builder.blank();
    builder.line("implementation");
    builder.blank();

    // Managed-FFI bodies (handle table, releaser, per-kind invoker
    // stubs, dispatcher classes, app helper).
    managed::emit_managed_implementation(&mut builder, ir, config, &targets);

    // Wrapper method bodies
    wrappers::generate_wrapper_implementation(&mut builder, ir, config)?;

    // Unit initialisation block — FPU mask, ABI self-check, releaser +
    // invoker stub registration.
    managed::emit_managed_initialization(&mut builder, ir);

    builder.line("end.");

    Ok(builder.finish())
}

fn emit_header(builder: &mut CodeBuilder) {
    builder.line("{ ============================================================================");
    builder.line("  Auto-generated Pascal (FPC/Lazarus) bindings for the Azul GUI framework.");
    builder.line("  Generated by azul-doc codegen v2 (lang_pascal).");
    builder.line("  DO NOT EDIT MANUALLY.");
    builder.line("");
    builder.line("  Compatible with Free Pascal Compiler 3.2+ (unit in Delphi mode; user");
    builder.line("  programs may use mode objfpc or delphi).");
    builder.line("  PACKRECORDS C and PACKENUM 4 are critical: they force C-ABI struct");
    builder.line("  layout and 4-byte enums so values passed by value match the Rust");
    builder.line("  extern \"C\" ABI. Consumed wrapper objects (by-value self / args) free");
    builder.line("  themselves; see the wrapper class comments.");
    builder
        .line("  ============================================================================ }");
    builder.blank();
}

// ============================================================================
// Shared type-name helpers (used by submodules)
// ============================================================================

/// The Az-prefixed FFI type name for a given IR type (e.g. `Dom` -> `AzDom`).
pub fn ffi_type_name(name: &str) -> String {
    format!("Az{}", name)
}

/// The Pascal record type name for a given IR type (e.g. `Dom` -> `TAzDom`).
///
/// FPC convention prefixes user-defined record types with `T` so that
/// pointer aliases can use `P` without collision.
pub fn record_type_name(name: &str) -> String {
    format!("T{}", ffi_type_name(name))
}

/// The Pascal pointer-type name for a given IR type (e.g. `Dom` -> `PAzDom`).
pub fn pointer_type_name(name: &str) -> String {
    format!("P{}", ffi_type_name(name))
}

/// Map a Rust/IR type name to its Pascal equivalent.
///
/// Pointers and references are turned into the matching `PAzFoo` typed
/// pointer when the inner type is a known IR type, or `Pointer` (untyped)
/// otherwise. Primitives map to `ctypes` aliases (`cint32`, `cfloat`, ...)
/// so the binding is correct on both 32- and 64-bit hosts.
pub fn map_type_to_pascal(rust_type: &str, ir: &CodegenIR) -> String {
    let trimmed = rust_type.trim();

    // Pointers and references — turn into a typed P-pointer when possible.
    if let Some(rest) = trimmed.strip_prefix("*const ") {
        return ptr_to_pascal(rest, ir);
    }
    if let Some(rest) = trimmed.strip_prefix("*mut ") {
        return ptr_to_pascal(rest, ir);
    }
    if let Some(rest) = trimmed.strip_prefix("&mut ") {
        return ptr_to_pascal(rest, ir);
    }
    if let Some(rest) = trimmed.strip_prefix('&') {
        return ptr_to_pascal(rest, ir);
    }

    // Arrays: `[T; N]` -> `array[0..N-1] of <PascalT>`
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        let inner = &trimmed[1..trimmed.len() - 1];
        if let Some(semi) = inner.rfind(';') {
            let elem = inner[..semi].trim();
            if let Ok(count) = inner[semi + 1..].trim().parse::<usize>() {
                let pas_elem = map_type_to_pascal(elem, ir);
                if count == 0 {
                    return format!("array[0..0] of {}", pas_elem);
                }
                return format!("array[0..{}] of {}", count - 1, pas_elem);
            }
        }
    }

    match trimmed {
        // Void / unit
        "void" | "c_void" | "()" => "Pointer".to_string(),

        // Booleans — Rust `bool` and `GLboolean` are both 1 byte (C99
        // `_Bool` semantics: 0 = false, nonzero = true). FPC's
        // `ctypes.cbool` is `LongBool` (4 bytes, Win32 BOOL flavour),
        // which would corrupt every struct that embeds a bool. Use
        // `ByteBool` (1 byte) for layout correctness.
        "bool" | "GLboolean" => "ByteBool".to_string(),

        // Signed / unsigned integers via ctypes (size-correct on all platforms)
        "i8" | "c_char" | "char" => "cint8".to_string(),
        "u8" | "c_uchar" => "cuint8".to_string(),
        "i16" => "cint16".to_string(),
        "u16" => "cuint16".to_string(),
        "i32" | "c_int" | "GLint" | "GLsizei" => "cint32".to_string(),
        "u32" | "c_uint" | "GLuint" | "GLenum" | "GLbitfield" => "cuint32".to_string(),
        "i64" | "GLint64" => "cint64".to_string(),
        "u64" | "GLuint64" => "cuint64".to_string(),
        "f32" | "GLfloat" | "GLclampf" => "cfloat".to_string(),
        "f64" | "GLdouble" | "GLclampd" => "cdouble".to_string(),
        "usize" | "size_t" | "uintptr_t" => "csize_t".to_string(),
        // FPC's `ctypes` provides `csize_t` but not `cssize_t`. We
        // ourselves alias to `PtrInt` which is the native signed
        // pointer-sized integer (matches Rust's isize).
        "isize" | "ssize_t" | "intptr_t" | "GLsizeiptr" | "GLintptr" => "PtrInt".to_string(),

        // Anything else: assume it's a known IR type and emit a record name.
        _ => {
            if ir.find_struct(trimmed).is_some()
                || ir.find_enum(trimmed).is_some()
                || ir.find_type_alias(trimmed).is_some()
                || ir.callback_typedefs.iter().any(|c| c.name == trimmed)
            {
                record_type_name(trimmed)
            } else {
                // Unknown -> opaque pointer.
                "Pointer".to_string()
            }
        }
    }
}

/// Helper: turn `<inner>` (the part after `*const`/`*mut`/`&`/`&mut`) into a
/// Pascal pointer expression. Tries the typed `PAzFoo` when the inner is a
/// known IR type; falls back to `PChar` for `c_char`/`char` and to plain
/// untyped `Pointer` otherwise.
fn ptr_to_pascal(inner: &str, ir: &CodegenIR) -> String {
    let inner = inner.trim();
    match inner {
        "c_char" | "char" | "i8" | "u8" => "PChar".to_string(),
        "c_void" | "void" | "()" => "Pointer".to_string(),
        _ => {
            if ir.find_struct(inner).is_some()
                || ir.find_enum(inner).is_some()
                || ir.find_type_alias(inner).is_some()
            {
                pointer_type_name(inner)
            } else {
                "Pointer".to_string()
            }
        }
    }
}

/// Sanitize a name for use as a Pascal identifier. Pascal reserved words
/// get a trailing underscore; this keeps the symbol unambiguous without
/// shadowing FPC keywords.
pub fn sanitize_identifier(name: &str) -> String {
    if is_pascal_reserved(name) || is_pascal_method_shadow(name) {
        return format!("{}_", name);
    }
    name.to_string()
}

/// Names that — although not Pascal keywords — collide with common
/// methods we emit on wrapper classes (`Len`, `Capacity`, `Clone`) or
/// with the implicit function result variable (`Result`: every wrapper
/// method is a function since the fluent rules, so an api.json argument
/// named `result` would shadow it). Pascal is case-insensitive, so a
/// parameter named `len` clashes with the `Len` method on the enclosing
/// class even though the casing differs. Suffix with `_` to disambiguate.
fn is_pascal_method_shadow(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "result"
            | "len"
            | "cap"
            | "clone"
            | "create"
            | "delete"
            | "free"
            | "raw"
            | "wrap"
            | "destroy"
            | "ptr"
            | "string"
            | "self"
            | "tag"
            | "get"
            | "set"
            | "id"
            | "value"
    )
}

/// Pascal reserved words (FPC + Object Pascal). Subset that's likely to
/// collide with field/argument names in api.json.
pub(super) fn is_pascal_reserved(name: &str) -> bool {
    matches!(
        name,
        "absolute"
            | "and"
            | "array"
            | "as"
            | "asm"
            | "begin"
            | "case"
            | "class"
            | "const"
            | "constructor"
            | "destructor"
            | "div"
            | "do"
            | "downto"
            | "else"
            | "end"
            | "except"
            | "exports"
            | "external"
            | "file"
            | "finalization"
            | "finally"
            | "for"
            | "function"
            | "goto"
            | "if"
            | "implementation"
            | "in"
            | "inherited"
            | "initialization"
            | "inline"
            | "interface"
            | "is"
            | "label"
            | "library"
            | "mod"
            | "nil"
            | "not"
            | "object"
            | "of"
            | "on"
            | "operator"
            | "or"
            | "out"
            | "packed"
            | "procedure"
            | "program"
            | "property"
            | "raise"
            | "record"
            | "repeat"
            | "resourcestring"
            | "self"
            | "set"
            | "shl"
            | "shr"
            | "string"
            | "then"
            | "threadvar"
            | "to"
            | "try"
            | "type"
            | "unit"
            | "until"
            | "uses"
            | "var"
            | "while"
            | "with"
            | "xor"
    )
}

/// Convert a snake_case or lowerCamelCase name to PascalCase (idiomatic
/// Pascal method names use PascalCase / CamelCase like the rest of the
/// language).
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
