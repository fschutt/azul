//! Fortran (modern, F2003+) binding generator.
//!
//! Generates a single `azul.f90` module file that:
//!
//! 1. Declares all C-ABI types as Fortran `type, bind(C)` derived types
//!    (the Fortran spelling of a C struct), unit-only enums as
//!    `enum, bind(C)` blocks (F2008) plus a matching `integer(c_int)`
//!    type alias, and tagged unions as derived types with an
//!    `integer(c_int) :: tag` discriminant plus a `type(c_ptr)` payload
//!    (Fortran has no native `union` so users cast the payload manually).
//! 2. Declares every C-API function inside an `interface ... end interface`
//!    block, each with the verbatim C symbol carried via
//!    `bind(C, name="AzFoo_create")`. Fortran is case-insensitive for
//!    its own identifiers, but the `name="..."` argument is case-sensitive
//!    so the linker matches the same exported symbols as the C/C++/Pascal
//!    bindings.
//! 3. Wraps every type that owns heap memory (i.e. has a matching
//!    `<TypeName>_delete` C function) in an idiomatic Fortran derived type
//!    with `final ::` finalizer (F2003+) that calls the matching `_delete`
//!    automatically when the value goes out of scope. Type-bound procedures
//!    (`procedure :: run => app_run`) provide the OO-style call syntax.
//! 4. Drops the `Az` prefix from user-facing wrapper names while keeping
//!    `Az`-prefixed FFI types and C symbols. Static factory functions are
//!    spelled `App_create`; instance methods are emitted as subroutines
//!    whose first argument is the wrapper type (Fortran TBPs).
//!
//! # Output structure (high-level)
//!
//! ```fortran
//! module azul
//!   use, intrinsic :: iso_c_binding
//!   implicit none
//!   private
//!
//!   ! --- Public exports ---
//!   public :: AzAppConfig, App, ButtonType_Primary, ...
//!
//!   ! --- POD derived types ---
//!   type, bind(C) :: AzAppConfig
//!     ! ... fields ...
//!   end type AzAppConfig
//!
//!   ! --- Unit enums (F2008 enum block + integer alias) ---
//!   enum, bind(C)
//!     enumerator :: AzButtonType_Primary = 0
//!     enumerator :: AzButtonType_Secondary = 1
//!   end enum
//!
//!   ! --- Tagged unions: tag + payload pointer the user casts manually ---
//!   type, bind(C) :: AzOptionI64
//!     integer(c_int) :: tag
//!     integer(c_int64_t) :: payload
//!   end type AzOptionI64
//!
//!   ! --- C ABI interface block ---
//!   interface
//!     function az_app_create(data, config) bind(C, name="AzApp_create") result(r)
//!       import
//!       type(AzRefAny), value :: data
//!       type(AzAppConfig), value :: config
//!       type(AzApp) :: r
//!     end function az_app_create
//!     ! ...
//!   end interface
//!
//!   ! --- Idiomatic wrappers with `final` finalizers ---
//!   type :: App
//!     private
//!     type(AzApp) :: raw
//!     logical :: owned = .true.
//!   contains
//!     final :: app_finalizer
//!     procedure :: run => app_run
//!   end type App
//!
//! contains
//!
//!   subroutine app_finalizer(self)
//!     type(App), intent(inout) :: self
//!     if (self%owned) call az_app_delete(self%raw)
//!   end subroutine app_finalizer
//!
//!   ! ... method bodies ...
//! end module azul
//! ```
//!
//! # Build
//!
//! ```bash
//! gfortran -c azul.f90
//! gfortran main.f90 azul.o -L. -lazul -o main
//! ```
//!
//! No standardized package manifest exists in the Fortran ecosystem, so
//! the orchestrator emits a plain Makefile beside the example
//! (`examples/fortran/Makefile`) rather than something like a `.fpm.toml`.
//!
//! # Wiring
//!
//! Like other Wave-2 generators this module is intentionally NOT wired from
//! `v2/mod.rs`. The orchestrator adds `pub mod lang_fortran;` plus a
//! `pub fn generate_fortran(api_data) -> Result<String>` helper that
//! mirrors `generate_pascal`, then writes the result to
//! `target/codegen/v2/azul.f90` and the Makefile from
//! [`makefile::generate_makefile`] alongside it.

use anyhow::Result;

use super::config::CodegenConfig;
use super::generator::CodeBuilder;
use super::ir::CodegenIR;

pub mod functions;
pub mod makefile;
pub mod managed;
pub mod types;
pub mod wrappers;

/// Library name used in `-lazul` link flags.
///
/// Matches the prebuilt artifact's name without extension; gfortran
/// resolves `-lazul` to `azul.dll` on Windows, `libazul.so` on Linux,
/// and `libazul.dylib` on macOS.
pub const LIB_NAME: &str = "azul";

/// Maximum identifier length permitted by F2003. F2008 raised this to
/// 63 chars; we use 63 throughout because all current toolchains accept
/// it. Names longer than this are truncated by [`truncate_identifier`].
pub const MAX_IDENT_LEN: usize = 63;

/// Public entry point. Produces the full `azul.f90` module source.
///
/// The caller is expected to write the result to disk
/// (e.g. `target/codegen/v2/azul.f90`) and to separately emit the
/// accompanying Makefile via [`makefile::generate_makefile`].
pub fn generate(ir: &CodegenIR, config: &CodegenConfig) -> Result<String> {
    let mut builder = CodeBuilder::new("  ");

    emit_header(&mut builder);

    // Module declaration + iso_c_binding import.
    builder.line("module azul");
    builder.indent();
    builder.line("use, intrinsic :: iso_c_binding");
    builder.line("implicit none");
    builder.line("private");
    builder.blank();

    // 1. Derived-type / enum / callback procedural type emission.
    types::generate_types(&mut builder, ir, config)?;

    // 2. C-ABI interface block.
    functions::generate_externals(&mut builder, ir, config)?;

    // 3. Idiomatic wrapper type declarations (still inside the module
    //    decl section — Fortran modules separate type declarations from
    //    procedure bodies via the `contains` keyword below).
    wrappers::generate_wrapper_decls(&mut builder, ir, config)?;

    // 3b. Managed-FFI host-invoker plumbing — module-level state + FFI
    //     interface declarations. Must come before `contains`.
    managed::emit_managed_decls(&mut builder, ir);

    // === module body (procedure implementations) ===
    builder.dedent();
    builder.line("contains");
    builder.indent();
    builder.blank();

    wrappers::generate_wrapper_bodies(&mut builder, ir, config)?;

    // Managed-FFI bodies (handle table accessors, releaser stub,
    // azul_refany_create / azul_refany_get).
    managed::emit_managed_bodies(&mut builder, ir);

    builder.dedent();
    builder.line("end module azul");

    Ok(builder.finish())
}

fn emit_header(builder: &mut CodeBuilder) {
    builder.line("! ============================================================================");
    builder.line("! Auto-generated Fortran (F2003+) bindings for the Azul GUI framework.");
    builder.line("! Generated by azul-doc codegen v2 (lang_fortran).");
    builder.line("! DO NOT EDIT MANUALLY.");
    builder.line("!");
    builder.line("! Requires Fortran 2003 or newer (uses iso_c_binding + final subroutines).");
    builder.line("! F2008 `enum, bind(C)` blocks are used for unit enums; supported by");
    builder.line("! gfortran >= 4.6 and Intel Fortran (ifort/ifx) >= 14.");
    builder.line("!");
    builder.line("! Build:");
    builder.line("!   gfortran -c azul.f90");
    builder.line("!   gfortran main.f90 azul.o -L. -lazul -o main");
    builder.line("! ============================================================================");
    builder.blank();
}

// ============================================================================
// Shared name helpers (used by submodules)
// ============================================================================

/// The `Az`-prefixed FFI derived-type name for an IR type
/// (e.g. `Dom` -> `AzDom`). Matches the Rust C-ABI struct name and the
/// linker symbol prefix.
pub fn ffi_type_name(name: &str) -> String {
    format!("Az{}", name)
}

/// Idiomatic, `Az`-stripped wrapper type name (e.g. `Dom` -> `Dom`).
/// Used for the user-facing F2003 derived type with `final ::`.
pub fn wrapper_type_name(name: &str) -> String {
    name.to_string()
}

/// The "instance method" prefix for wrapper procedures. We use a
/// snake_case lowering of the wrapper type as the prefix so type-bound
/// procedure resolution works without conflicts: `App_run` -> `app_run`.
pub fn instance_method_prefix(wrapper_type: &str) -> String {
    pascal_to_snake_case(wrapper_type)
}

/// Map a Rust/IR type name to its Fortran `iso_c_binding` equivalent.
///
/// References / pointers always resolve to `type(c_ptr)` because Fortran
/// has no typed-pointer aliases the way Pascal/C do; users rely on the
/// derived-type definition for layout and on `c_loc`/`c_f_pointer` for
/// raw pointer interop.
///
/// IR derived types (structs, enums, callback typedefs) resolve to
/// `type(AzFoo)` so they may be used inline as fields or arguments.
pub fn map_type_to_fortran(rust_type: &str, ir: &CodegenIR) -> String {
    let trimmed = rust_type.trim();

    // Pointers and references — always `type(c_ptr)` (Fortran has no
    // typed-pointer alias). The wrapper layer reinterprets via
    // `c_f_pointer` when needed.
    if trimmed.starts_with("*const ")
        || trimmed.starts_with("*mut ")
        || trimmed.starts_with("&mut ")
        || trimmed.starts_with('&')
    {
        return "type(c_ptr)".to_string();
    }

    // Arrays: `[T; N]` -> `<elem> dimension(N)` would be the natural
    // mapping, but `bind(C)` only allows fixed-size arrays of basic
    // intrinsic types here, and we don't currently see user-facing arrays
    // in api.json. Fall back to opaque `type(c_ptr)` for safety.
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        return "type(c_ptr)".to_string();
    }

    match trimmed {
        // Void / unit: there is no `void` in Fortran; for arguments this
        // path is unreachable (we emit `subroutine` instead of `function`).
        // For pointer-to-void use `type(c_ptr)`.
        "void" | "c_void" | "()" => "type(c_ptr)".to_string(),

        // Booleans
        "bool" | "GLboolean" => "logical(c_bool)".to_string(),

        // Signed / unsigned integers via iso_c_binding kind selectors.
        // Fortran has no unsigned-integer kind, so `u8`/`u16`/... map to
        // the same kind as their signed counterparts; this is the same
        // approach the Pascal/Ada bindings take.
        "i8" | "u8" | "c_char" | "char" | "c_uchar" => "integer(c_int8_t)".to_string(),
        "i16" | "u16" => "integer(c_int16_t)".to_string(),
        "i32" | "u32" | "c_int" | "c_uint" | "GLint" | "GLuint" | "GLenum" | "GLbitfield"
        | "GLsizei" => "integer(c_int32_t)".to_string(),
        "i64" | "u64" | "GLint64" | "GLuint64" => "integer(c_int64_t)".to_string(),
        "f32" | "GLfloat" | "GLclampf" => "real(c_float)".to_string(),
        "f64" | "GLdouble" | "GLclampd" => "real(c_double)".to_string(),
        "usize" | "size_t" | "uintptr_t" => "integer(c_size_t)".to_string(),
        "isize" | "ssize_t" | "intptr_t" | "GLsizeiptr" | "GLintptr" => {
            "integer(c_intptr_t)".to_string()
        }

        // Anything else: assume it's a known IR type and emit a
        // `type(AzFoo)`. Unit enums (no payload variants) are emitted
        // as `enumerator :: AzFoo_X = N` constants without a backing
        // derived type, so they must map to `integer(c_int)` here.
        // Tagged-union enums DO get a `type, bind(C) :: AzFoo` block.
        // Unknown types fall back to `type(c_ptr)` so the generated
        // module still compiles.
        _ => {
            if let Some(e) = ir.find_enum(trimmed) {
                // Skipped enum categories produce no derived type;
                // callers should treat them as opaque pointers.
                if matches!(
                    e.category,
                    crate::codegen::v2::ir::TypeCategory::Recursive
                        | crate::codegen::v2::ir::TypeCategory::VecRef
                        | crate::codegen::v2::ir::TypeCategory::DestructorOrClone
                        | crate::codegen::v2::ir::TypeCategory::GenericTemplate
                ) || !e.generic_params.is_empty()
                {
                    "type(c_ptr)".to_string()
                } else if e.is_union {
                    format!("type({})", ffi_type_name(trimmed))
                } else {
                    "integer(c_int)".to_string()
                }
            } else if ir.callback_typedefs.iter().any(|c| c.name == trimmed) {
                // Callback typedefs are emitted as `abstract interface`
                // signatures + a `procedure pointer` value; the type itself
                // is `c_funptr` from `iso_c_binding`. Using `type(AzFoo)`
                // here would dangle — there's no derived type by that
                // name.
                "type(c_funptr)".to_string()
            } else if let Some(ta) = ir.find_type_alias(trimmed) {
                // Simple type alias: resolve to the target. The codegen
                // only emits a `type, bind(C) :: AzFoo` block for
                // monomorphized aliases; simple aliases (ScanCode = u32,
                // GLuint = u32) have no derived type and must lower to
                // the target's representation. Recurse so chains resolve.
                if ta.monomorphized_def.is_some() {
                    format!("type({})", ffi_type_name(trimmed))
                } else {
                    map_type_to_fortran(&ta.target, ir)
                }
            } else if let Some(s) = ir.find_struct(trimmed) {
                // Skipped struct categories (Recursive, VecRef,
                // DestructorOrClone, GenericTemplate) have no `type,
                // bind(C) :: AzFoo` declaration emitted, so referencing
                // their derived-type name would dangle. Fall back to
                // `type(c_ptr)` for those — callers treat them as
                // opaque references.
                if matches!(
                    s.category,
                    crate::codegen::v2::ir::TypeCategory::Recursive
                        | crate::codegen::v2::ir::TypeCategory::VecRef
                        | crate::codegen::v2::ir::TypeCategory::DestructorOrClone
                        | crate::codegen::v2::ir::TypeCategory::GenericTemplate
                ) || !s.generic_params.is_empty()
                {
                    "type(c_ptr)".to_string()
                } else {
                    format!("type({})", ffi_type_name(trimmed))
                }
            } else {
                "type(c_ptr)".to_string()
            }
        }
    }
}

/// Sanitize a name for use as a Fortran identifier.
///
/// - Reserved keywords get a trailing underscore.
/// - Names longer than [`MAX_IDENT_LEN`] are truncated.
/// - Leading underscores (illegal in Fortran identifiers) get prefixed
///   with `f_`.
pub fn sanitize_identifier(name: &str) -> String {
    let mut out = if is_fortran_reserved(name) {
        format!("{}_", name)
    } else if name.starts_with('_') {
        format!("f{}", name)
    } else {
        name.to_string()
    };
    if out.len() > MAX_IDENT_LEN {
        out = truncate_identifier(&out);
    }
    out
}

/// Fortran reserved words that are likely to collide with field /
/// argument names in api.json. The full list is much larger (>100); we
/// include only those plausibly emitted from user-facing field names.
pub fn is_fortran_reserved(name: &str) -> bool {
    matches!(
        name.to_lowercase().as_str(),
        "if" | "then"
            | "else"
            | "elseif"
            | "endif"
            | "do"
            | "enddo"
            | "end"
            | "function"
            | "subroutine"
            | "module"
            | "program"
            | "type"
            | "interface"
            | "use"
            | "implicit"
            | "real"
            | "integer"
            | "logical"
            | "character"
            | "complex"
            | "double"
            | "precision"
            | "kind"
            | "len"
            | "where"
            | "elsewhere"
            | "endwhere"
            | "select"
            | "case"
            | "default"
            | "endselect"
            | "go"
            | "goto"
            | "continue"
            | "stop"
            | "return"
            | "call"
            | "pure"
            | "elemental"
            | "recursive"
            | "result"
            | "contains"
            | "private"
            | "public"
            | "protected"
            | "save"
            | "data"
            | "block"
            | "common"
            | "equivalence"
            | "namelist"
            | "external"
            | "intrinsic"
            | "optional"
            | "parameter"
            | "pointer"
            | "target"
            | "allocatable"
            | "dimension"
            | "intent"
            | "in"
            | "out"
            | "inout"
            | "value"
            | "volatile"
            | "asynchronous"
            | "bind"
            | "import"
            | "abstract"
            | "class"
            | "deferred"
            | "extends"
            | "final"
            | "generic"
            | "non_overridable"
            | "nopass"
            | "pass"
            | "procedure"
            | "sequence"
            | "abs"
            | "min"
            | "max"
            | "size"
            | "data_"
            // Single-letter names that collide with the synthetic
            // `r` result variable on every generated function.
            // Without these the Fortran compiler raises
            // "DUMMY attribute conflicts with RESULT attribute".
            | "r"
    )
}

/// Truncate an identifier to [`MAX_IDENT_LEN`] characters by hashing the
/// tail and appending a short suffix; this keeps the prefix readable
/// while guaranteeing uniqueness.
pub fn truncate_identifier(name: &str) -> String {
    if name.len() <= MAX_IDENT_LEN {
        return name.to_string();
    }
    // Reserve 9 chars for "_" + 8-char hex hash. Take the first
    // (MAX_IDENT_LEN - 9) chars verbatim, then append `_<hash>`.
    let head_len = MAX_IDENT_LEN.saturating_sub(9);
    let head: String = name.chars().take(head_len).collect();

    let mut hash: u32 = 0x811c9dc5;
    for b in name.as_bytes() {
        hash ^= *b as u32;
        hash = hash.wrapping_mul(0x01000193);
    }
    format!("{}_{:08x}", head, hash)
}

/// Convert PascalCase / CamelCase to snake_case (lowercase). Used to
/// derive instance-method prefixes from wrapper type names
/// (`AppConfig` -> `app_config`).
pub fn pascal_to_snake_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for (i, c) in s.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// Sanitize a doc comment for inclusion as a Fortran `! ...` line.
/// Newlines split the comment over multiple `!` lines; the caller is
/// responsible for emitting one `!` prefix per resulting line.
pub fn sanitize_comment_line(s: &str) -> String {
    s.replace('\r', " ")
        .replace('\n', " ")
        .trim()
        .to_string()
}
