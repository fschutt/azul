//! OCaml binding generator.
//!
//! Produces two OCaml compilation units:
//!
//! 1. `azul.mli` — the module interface: type declarations, opaque
//!    abstract record types for wrappers, FFI value signatures (the
//!    `foreign` declarations), and the public surface of the idiomatic
//!    `Azul` module nest.
//! 2. `azul.ml` — the module implementation: `open Ctypes`, `open
//!    Foreign`, library load, struct definitions (with `field` / `seal`),
//!    function bindings via `foreign`, and the wrapper-record
//!    constructors that attach a `Gc.finalise` hook to call the matching
//!    `_delete` C function.
//!
//! ## Surface
//!
//! - All FFI-level identifiers are emitted in `lower_snake_case`
//!   (OCaml's value/type-name convention) — e.g. `az_app`, `az_app_create`.
//! - The `foreign "<C symbol>" (...)` link name uses the **exact** C
//!   symbol from the IR (`AzApp_create`), never the OCaml-snake form.
//! - Idiomatic surface lives inside nested modules: `Azul.App.create`,
//!   `Azul.App.run`, etc. The `Az_` / `Az` prefix is dropped.
//! - Tagged-union enums are surfaced as polymorphic variants
//!   (`[ \`None | \`Some of int64 ]`) with `to_ffi` / `of_ffi`
//!   conversion functions.
//! - Wrapper records:
//!
//!   ```ocaml
//!   type app = { raw : <ffi_struct>; mutable disposed : bool }
//!   let make_app raw =
//!     let r = { raw; disposed = false } in
//!     Gc.finalise
//!       (fun a -> if not a.disposed then begin
//!          az_app_delete (Ctypes.addr a.raw); a.disposed <- true
//!        end)
//!       r;
//!     r
//!   ```
//!
//! ## Output protocol
//!
//! `generate(ir, config)` returns a single `String` containing BOTH
//! files separated by [`SPLIT_MARKER`] on its own line:
//!
//! ```text
//! <azul.mli contents>
//! (*==SPLIT==*)
//! <azul.ml contents>
//! ```
//!
//! The marker is itself a syntactically valid OCaml block comment, so
//! even if a downstream tool fails to split the file, the combined text
//! still parses (the marker becomes a no-op comment line). The
//! orchestrator splits on the marker and writes each half to its
//! respective file.
//!
//! Returning a single `String` matches the signature shape used by every
//! other v2 language entry point (Python, C#, Ruby, Lua, Ada).
//!
//! Keep [`SPLIT_MARKER`] stable; it is part of the contract with the
//! orchestrator.

pub mod dune;
pub mod functions;
pub mod managed;
pub mod types;
pub mod wrappers;

use anyhow::Result;

use super::config::CodegenConfig;
use super::generator::CodeBuilder;
use super::ir::CodegenIR;

/// Library link name passed to `Dl.dlopen` and used by `foreign` to
/// resolve the prebuilt artifact at runtime.
pub const LIB_NAME: &str = "azul";

/// Separator between `azul.mli` and `azul.ml` contents in the single
/// returned `String`. The marker is a valid OCaml block comment so
/// the combined file remains parseable if accidentally not split.
pub const SPLIT_MARKER: &str = "(*==SPLIT==*)";

/// Public entry point. Generates the full `azul.mli` and `azul.ml` text
/// concatenated with [`SPLIT_MARKER`] between them.
pub fn generate(ir: &CodegenIR, config: &CodegenConfig) -> Result<String> {
    let interface = generate_interface(ir, config)?;
    let implementation = generate_implementation(ir, config)?;

    let mut out =
        String::with_capacity(interface.len() + implementation.len() + SPLIT_MARKER.len() + 2);
    out.push_str(&interface);
    if !interface.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(SPLIT_MARKER);
    out.push('\n');
    out.push_str(&implementation);
    Ok(out)
}

/// Build the module interface (`azul.mli`).
fn generate_interface(ir: &CodegenIR, config: &CodegenConfig) -> Result<String> {
    let mut builder = CodeBuilder::new(&config.indent);

    interface_header(&mut builder);

    // Types section: opaque type stubs for FFI structs/enums plus
    // polymorphic-variant signatures for tagged unions.
    types::emit_interface_types(&mut builder, ir, config)?;

    // Managed-FFI public helper signatures. Must come BEFORE wrappers
    // so the wrappers can reference azul_refany_create etc.
    managed::emit_managed_interface(&mut builder, ir);

    // Wrapper record types declared as abstract: the consumer never sees
    // the field shape (the `mutable disposed` flag is implementation
    // detail); they get a `make_t` smart constructor signature.
    wrappers::emit_wrapper_interface(&mut builder, ir, config)?;

    // Idiomatic Azul module surface: nested submodules per class.
    wrappers::emit_idiomatic_module_interface(&mut builder, ir, config)?;

    Ok(builder.finish())
}

/// Build the module implementation (`azul.ml`).
fn generate_implementation(ir: &CodegenIR, config: &CodegenConfig) -> Result<String> {
    let mut builder = CodeBuilder::new(&config.indent);

    implementation_header(&mut builder);
    implementation_preamble(&mut builder);

    // 1. Forward struct typ declarations so mutually-recursive references
    //    resolve. Each struct emits an opaque `type` plus a `structure`
    //    typ value; fields are added in the next pass.
    types::emit_forward_struct_decls(&mut builder, ir, config);

    // 2. Field definitions + seal for each struct, plus enum constants
    //    and tagged-union accessor scaffolding.
    types::emit_struct_fields_and_enums(&mut builder, ir, config)?;

    // 3. Raw `foreign` bindings, one per IR FunctionDef.
    functions::emit_foreign_bindings(&mut builder, ir, config)?;

    // 3b. Managed-FFI runtime helpers (host-invoker prelude). Lands
    //     before the wrapper records so wrappers may call
    //     azul_refany_create / register_callback.
    managed::emit_managed_prelude(&mut builder, ir);

    // 4. Wrapper records + Gc.finalise smart constructors.
    wrappers::emit_wrapper_records(&mut builder, ir, config)?;

    // 5. Idiomatic Azul module surface implementation.
    wrappers::emit_idiomatic_module_implementation(&mut builder, ir, config)?;

    Ok(builder.finish())
}

fn interface_header(builder: &mut CodeBuilder) {
    builder.line("(* ============================================================================");
    builder.line(" * Auto-generated OCaml bindings for the Azul GUI framework (interface).");
    builder.line(" * Generated by azul-doc codegen v2 (lang_ocaml). DO NOT EDIT MANUALLY.");
    builder.line(" * ============================================================================ *)");
    builder.blank();
}

fn implementation_header(builder: &mut CodeBuilder) {
    builder.line("(* ============================================================================");
    builder.line(" * Auto-generated OCaml bindings for the Azul GUI framework (implementation).");
    builder.line(" * Generated by azul-doc codegen v2 (lang_ocaml). DO NOT EDIT MANUALLY.");
    builder.line(" * ============================================================================ *)");
    builder.blank();
}

fn implementation_preamble(builder: &mut CodeBuilder) {
    builder.line("open Ctypes");
    builder.line("open Foreign");
    builder.blank();
    builder.line("(* Force the dynamic loader to bring in libazul up-front so all `foreign`");
    builder.line("   lookups below resolve from the same handle. RTLD_GLOBAL lets the");
    builder.line("   library's transitive dependencies (e.g. system OpenGL) link too.");
    builder.line("   Try each platform-conventional filename in turn so the binding");
    builder.line("   loads on Linux, macOS, and Windows without manual configuration.");
    builder.line("   The first match wins; failures are silenced (logged via stderr).");
    builder.line("   Users can override the search by setting AZ_DYLIB. *)");
    builder.line("let () =");
    builder.indent();
    builder.line("let candidates = match Sys.getenv_opt \"AZ_DYLIB\" with");
    builder.indent();
    builder.line("| Some p when String.length p > 0 -> [p]");
    builder.line("| _ -> [\"libazul.dylib\"; \"libazul.so\"; \"azul.dll\"; \"./libazul.dylib\"; \"./libazul.so\"]");
    builder.dedent();
    builder.line("in");
    builder.line("let rec try_load = function");
    builder.indent();
    builder.line("| [] -> ()");
    builder.line("| candidate :: rest ->");
    builder.indent();
    builder.line("(try ignore (Dl.dlopen ~filename:candidate ~flags:[Dl.RTLD_LAZY; Dl.RTLD_GLOBAL])");
    builder.line(" with Dl.DL_error _ -> try_load rest)");
    builder.dedent();
    builder.dedent();
    builder.line("in try_load candidates");
    builder.dedent();
    builder.blank();
}

// ============================================================================
// Shared helpers (used by submodules)
// ============================================================================

/// Convert an IR type name (`PascalCase`) to the OCaml FFI struct
/// identifier (`lower_snake_case` with the `az_` prefix).
///
/// Example: `App` -> `az_app`, `LayoutCallbackInfo` ->
/// `az_layout_callback_info`.
pub fn ocaml_ffi_type_name(name: &str) -> String {
    format!("az_{}", to_snake_case(name))
}

/// Convert an IR type name (`PascalCase`) to the user-facing wrapper
/// record name (`lower_snake_case`, no prefix). Shadow-prone names
/// get a `_wrapper` suffix instead of the `az_` prefix used at the
/// FFI level — `az_string` is already the FFI typ; the wrapper must
/// be a distinct identifier.
///
/// Example: `App` -> `app`; `String` -> `string_wrapper`.
pub fn ocaml_wrapper_type_name(name: &str) -> String {
    let snake = sanitize_identifier(&to_snake_case(name));
    if shadows_ocaml_primitive(&snake) {
        format!("{}_wrapper", snake)
    } else {
        snake
    }
}

fn shadows_ocaml_primitive(s: &str) -> bool {
    matches!(
        s,
        "string"
            | "bool"
            | "int"
            | "char"
            | "float"
            | "list"
            | "array"
            | "option"
            | "result"
            | "unit"
            | "bytes"
            | "ref"
            | "exn"
    )
}

/// Convert an IR type name (`PascalCase`) to the user-facing module
/// name in the idiomatic surface (`Pascal_Case` with submodule chain
/// preserved). The `Az` prefix is dropped at this layer.
///
/// Example: `App` -> `App`, `LayoutCallbackInfo` -> `Layout_callback_info`.
///
/// OCaml module names must be capitalised; we keep the first letter
/// upper and treat subsequent CamelHumps as snake-cased segments to
/// keep names short and readable.
pub fn ocaml_module_name(name: &str) -> String {
    // Module names in OCaml are conventionally UpperCamelCase
    // ("RefAny", "WindowCreateOptions"). The previous implementation
    // produced `Ref_any` (snake-with-leading-cap) which is legal but
    // doesn't match the hand-written hello-world's `RefAny.wrap`
    // calls and isn't idiomatic. Strip the leading `Az`/`Iface`
    // prefix if present, then upper-camel-case the rest.
    let body = name.strip_prefix("Az").unwrap_or(name);
    // Re-split on underscores (in case the input was snake) and on
    // case boundaries (in case it was already camel) so we get a
    // consistent sequence of words.
    let snake = to_snake_case(body);
    let mut out = String::with_capacity(snake.len());
    let mut at_word_start = true;
    for c in snake.chars() {
        if c == '_' {
            at_word_start = true;
            continue;
        }
        if at_word_start {
            out.extend(c.to_uppercase());
            at_word_start = false;
        } else {
            out.push(c);
        }
    }
    if out.is_empty() {
        "M".to_string()
    } else {
        out
    }
}

/// Map a Rust/IR type name (with optional pointer/reference prefix) to
/// the matching OCaml `Ctypes` view expression.
///
/// Pointer / reference variants collapse to `(ptr <inner>)` if the
/// inner type is known to the IR, otherwise to `(ptr void)`.
/// `*const c_char` becomes `string` (Ctypes does the C-string
/// marshalling).
/// Map a Rust IR type name to its OCaml-position TYPE string for use
/// in `val foo : T -> U` signatures inside the .mli interface. Where
/// Ctypes' value-level typ name and the corresponding OCaml type
/// differ (e.g. `uint8_t : uint8_t typ` is a value, but the OCaml
/// type alias is `Unsigned.UInt8.t`), return the OCaml type.
pub fn map_type_to_ocaml_typ(rust_type: &str, ir: &CodegenIR) -> String {
    let trimmed = rust_type.trim();

    // Pointer/reference forms — use the postfix type-position form.
    if let Some(rest) = trimmed.strip_prefix("*const ") {
        let inner = rest.trim();
        if inner == "c_char" || inner == "u8" {
            return "string".to_string();
        }
        return inner_pointer_form_type(inner, ir);
    }
    if let Some(rest) = trimmed.strip_prefix("*mut ") {
        return inner_pointer_form_type(rest.trim(), ir);
    }
    if let Some(rest) = trimmed.strip_prefix("&mut ") {
        return inner_pointer_form_type(rest.trim(), ir);
    }
    if let Some(rest) = trimmed.strip_prefix('&') {
        return inner_pointer_form_type(rest.trim(), ir);
    }

    match trimmed {
        "bool" => "bool".to_string(),
        // Sized integers — OCaml type names in `Unsigned.*` and
        // `Signed.*`, NOT the bare ctypes value names. `int`
        // suffices for any small width where preserving the exact
        // representation doesn't matter at the Haskell level —
        // the value-position emit handles the precise C ABI width.
        "u8" | "c_uchar" => "Unsigned.UInt8.t".to_string(),
        "i8" | "c_char" => "int".to_string(), // Signed.SInt8.t exists but `int` is more ergonomic
        "char" => "char".to_string(),
        "u16" => "Unsigned.UInt16.t".to_string(),
        "i16" => "int".to_string(),
        "u32" | "c_uint" => "Unsigned.UInt32.t".to_string(),
        "i32" | "c_int" => "int32".to_string(),
        "u64" => "Unsigned.UInt64.t".to_string(),
        "i64" => "int64".to_string(),
        "f32" => "float".to_string(),
        "f64" => "float".to_string(),
        // `size_t` value-position is the Ctypes typ. The actual OCaml
        // type is `Unsigned.Size_t.t`. Same for ptrdiff_t / isize.
        // Use the precise Ctypes module paths so the mli matches what
        // `foreign ... size_t @-> ... @-> returning ...` actually
        // returns.
        "usize" => "Unsigned.Size_t.t".to_string(),
        "isize" => "Ctypes.Ptrdiff.t".to_string(),
        "c_void" | "()" | "void" => "unit".to_string(),

        _ => {
            // Struct types passed by value across the FFI are
            // represented at the value level as `T Ctypes.structure`;
            // the type-position emit must say the same so the mli
            // signature matches the impl's actual return type.
            // EXCEPT for filtered-out categories (Recursive, VecRef,
            // DestructorOrClone) which the codegen emits as opaque
            // `type T = unit ptr` placeholders — those use the bare
            // name in both positions.
            if let Some(s) = ir.find_struct(trimmed) {
                if matches!(
                    s.category,
                    super::ir::TypeCategory::Recursive
                        | super::ir::TypeCategory::VecRef
                        | super::ir::TypeCategory::DestructorOrClone
                ) {
                    return ocaml_ffi_type_name(trimmed);
                }
                return format!("{} Ctypes.structure", ocaml_ffi_type_name(trimmed));
            }
            // Tagged unions are also `Ctypes.structure` (we model the
            // union via a payload byte-array inside a struct), unless
            // filtered.
            if let Some(e) = ir.find_enum(trimmed) {
                if matches!(
                    e.category,
                    super::ir::TypeCategory::Recursive
                        | super::ir::TypeCategory::VecRef
                        | super::ir::TypeCategory::DestructorOrClone
                ) {
                    return ocaml_ffi_type_name(trimmed);
                }
                if e.is_union {
                    return format!("{} Ctypes.structure", ocaml_ffi_type_name(trimmed));
                }
                // Unit enums are `int` aliases.
                return ocaml_ffi_type_name(trimmed);
            }
            if ir.find_type_alias(trimmed).is_some()
                || ir.callback_typedefs.iter().any(|c| c.name == trimmed)
            {
                ocaml_ffi_type_name(trimmed)
            } else {
                "(unit ptr)".to_string()
            }
        }
    }
}

pub fn map_type_to_ocaml(rust_type: &str, ir: &CodegenIR) -> String {
    let trimmed = rust_type.trim();

    // Pointer/reference forms.
    if let Some(rest) = trimmed.strip_prefix("*const ") {
        let inner = rest.trim();
        if inner == "c_char" || inner == "u8" {
            return "string".to_string();
        }
        return inner_pointer_form(inner, ir);
    }
    if let Some(rest) = trimmed.strip_prefix("*mut ") {
        return inner_pointer_form(rest.trim(), ir);
    }
    if let Some(rest) = trimmed.strip_prefix("&mut ") {
        return inner_pointer_form(rest.trim(), ir);
    }
    if let Some(rest) = trimmed.strip_prefix('&') {
        return inner_pointer_form(rest.trim(), ir);
    }

    match trimmed {
        // Primitives. We use Ctypes' explicit-width integer views so the
        // memory layout matches the C ABI on every host (OCaml's native
        // `int` is 63-bit on 64-bit systems, which would mis-align a
        // C `int` field).
        "bool" => "bool".to_string(),
        "u8" | "c_uchar" => "uint8_t".to_string(),
        "i8" | "c_char" => "int8_t".to_string(),
        "char" => "char".to_string(),
        "u16" => "uint16_t".to_string(),
        "i16" => "int16_t".to_string(),
        "u32" | "c_uint" => "uint32_t".to_string(),
        "i32" | "c_int" => "int32_t".to_string(),
        "u64" => "uint64_t".to_string(),
        "i64" => "int64_t".to_string(),
        "f32" => "float".to_string(),
        "f64" => "double".to_string(),
        "usize" => "size_t".to_string(),
        "isize" => "ptrdiff_t".to_string(),
        "c_void" | "()" | "void" => "void".to_string(),

        _ => {
            if ir.find_struct(trimmed).is_some()
                || ir.find_enum(trimmed).is_some()
                || ir.find_type_alias(trimmed).is_some()
                || ir.callback_typedefs.iter().any(|c| c.name == trimmed)
            {
                ocaml_ffi_type_name(trimmed)
            } else {
                "(ptr void)".to_string()
            }
        }
    }
}

/// Pointer-form for VALUE-level emission (inside `Ctypes` schema
/// expressions like `ptr T @-> ... @-> returning Y`). Uses prefix
/// `ptr T` which Ctypes' `ptr` function expects.
pub fn inner_pointer_form(inner: &str, ir: &CodegenIR) -> String {
    if inner.is_empty() || inner == "c_void" || inner == "void" || inner == "()" {
        return "(ptr void)".to_string();
    }
    if ir.find_struct(inner).is_some()
        || ir.find_enum(inner).is_some()
        || ir.find_type_alias(inner).is_some()
        || ir.callback_typedefs.iter().any(|c| c.name == inner)
    {
        format!("(ptr {})", ocaml_ffi_type_name(inner))
    } else {
        "(ptr void)".to_string()
    }
}

/// Pointer-form for TYPE-level emission (val signatures in .mli).
/// OCaml type syntax applies constructors postfix — `T ptr`, not
/// `ptr T`. The latter is rejected with "expects 0 argument(s), but
/// is here applied to 1".
///
/// For struct/union types, the actual runtime type is
/// `<name> Ctypes.structure Ctypes_static.ptr`; the .mli signature
/// must say the same so it matches what `ptr <name_typ>` produces
/// in the impl.
pub fn inner_pointer_form_type(inner: &str, ir: &CodegenIR) -> String {
    if inner.is_empty() || inner == "c_void" || inner == "void" || inner == "()" {
        return "(unit Ctypes_static.ptr)".to_string();
    }
    if let Some(s) = ir.find_struct(inner) {
        if matches!(
            s.category,
            super::ir::TypeCategory::Recursive
                | super::ir::TypeCategory::VecRef
                | super::ir::TypeCategory::DestructorOrClone
        ) {
            return format!("({} Ctypes_static.ptr)", ocaml_ffi_type_name(inner));
        }
        return format!(
            "({} Ctypes.structure Ctypes_static.ptr)",
            ocaml_ffi_type_name(inner)
        );
    }
    if let Some(e) = ir.find_enum(inner) {
        if matches!(
            e.category,
            super::ir::TypeCategory::Recursive
                | super::ir::TypeCategory::VecRef
                | super::ir::TypeCategory::DestructorOrClone
        ) {
            return format!("({} Ctypes_static.ptr)", ocaml_ffi_type_name(inner));
        }
        if e.is_union {
            return format!(
                "({} Ctypes.structure Ctypes_static.ptr)",
                ocaml_ffi_type_name(inner)
            );
        }
        return format!("({} Ctypes_static.ptr)", ocaml_ffi_type_name(inner));
    }
    if ir.find_type_alias(inner).is_some()
        || ir.callback_typedefs.iter().any(|c| c.name == inner)
    {
        format!("({} Ctypes_static.ptr)", ocaml_ffi_type_name(inner))
    } else {
        "(unit Ctypes_static.ptr)".to_string()
    }
}

/// Convert a `PascalCase` or `camelCase` name to `lower_snake_case`.
pub fn to_snake_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    let mut prev_lower_or_digit = false;
    for c in name.chars() {
        if c.is_ascii_uppercase() {
            if prev_lower_or_digit {
                out.push('_');
            }
            for low in c.to_lowercase() {
                out.push(low);
            }
            prev_lower_or_digit = false;
        } else if c == '_' {
            out.push('_');
            prev_lower_or_digit = false;
        } else {
            out.push(c);
            prev_lower_or_digit = c.is_ascii_lowercase() || c.is_ascii_digit();
        }
    }
    out
}

/// Sanitize an identifier that may collide with an OCaml reserved word.
/// We append a trailing underscore to mangle.
pub fn sanitize_identifier(name: &str) -> String {
    if is_ocaml_reserved(name) {
        format!("{}_", name)
    } else {
        // Also catch names starting with a digit (illegal in OCaml).
        if name
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
        {
            format!("_{}", name)
        } else {
            name.to_string()
        }
    }
}

fn is_ocaml_reserved(s: &str) -> bool {
    matches!(
        s,
        "and"
            | "as"
            | "assert"
            | "asr"
            | "begin"
            | "class"
            | "constraint"
            | "do"
            | "done"
            | "downto"
            | "else"
            | "end"
            | "exception"
            | "external"
            | "false"
            | "for"
            | "fun"
            | "function"
            | "functor"
            | "if"
            | "in"
            | "include"
            | "inherit"
            | "initializer"
            | "land"
            | "lazy"
            | "let"
            | "lor"
            | "lsl"
            | "lsr"
            | "lxor"
            | "match"
            | "method"
            | "mod"
            | "module"
            | "mutable"
            | "new"
            | "nonrec"
            | "object"
            | "of"
            | "open"
            | "or"
            | "private"
            | "rec"
            | "sig"
            | "struct"
            | "then"
            | "to"
            | "true"
            | "try"
            | "type"
            | "val"
            | "virtual"
            | "when"
            | "while"
            | "with"
            // OCaml 5+ added effect handlers; `effect` is now a
            // reserved keyword that produces a syntax error when used
            // as an identifier.
            | "effect"
    )
}

/// Sanitize a doc-comment line so a stray `*)` mid-string doesn't
/// terminate the surrounding OCaml block comment.
pub fn sanitize_doc(s: &str) -> String {
    s.replace('\n', " ")
        .replace("*)", "* )")
        .trim()
        .to_string()
}
