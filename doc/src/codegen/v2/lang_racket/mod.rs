//! Racket (`ffi/unsafe`) binding generator.
//!
//! Emits a single `azul.rkt` source file that exposes the C-ABI of
//! `libazul` to Racket programs via the built-in `ffi/unsafe` +
//! `ffi/unsafe/define` libraries. Structurally this is the closest
//! sibling of the Common Lisp (CFFI) generator (`lang_lisp`): both are
//! Lisps with a *declarative* FFI (declare types, declare functions,
//! load the library) rather than the C-header-embedding style of the
//! LuaJIT generator.
//!
//! ## File structure (top to bottom)
//!
//! 1. `#lang racket/base` banner + `(require ffi/unsafe
//!    ffi/unsafe/define)`.
//! 2. `(define azul-lib (ffi-lib ...))` — locate
//!    `libazul.{so,dylib}` / `azul.dll`, honouring `AZ_LIB_DIR`.
//! 3. `(define-ffi-definer define-azul azul-lib ...)` — the binding
//!    macro every `Az*` function is declared through.
//! 4. Type layer (see [`types`]):
//!    - unit enums → integer `(define AzUpdate_RefreshDom 1)` constants
//!      + a `(define _AzUpdate _uint32)` ctype alias.
//!    - POD structs → `(define-cstruct _AzFoo ([field _uint32] ...))`.
//!    - tagged unions → per-variant `define-cstruct` (each leading with
//!      the `tag` slot, exactly like the C ABI's tag-then-payload
//!      layout) + a wrapping `(define _AzFoo (_union ...))`.
//!    - callback typedefs → `(define _AzFooCallbackType _fpointer)`.
//! 5. Function layer (see [`functions`]): one `(define-azul AzApp_create
//!    (_fun _AzRefAny _AzAppConfig -> _AzApp))` per IR function.
//! 6. Managed-FFI runtime (see [`managed`]): the host-invoker plumbing,
//!    `register-callback`, and the `refany-create` / `refany-get`
//!    user-data helpers.
//! 7. Idiomatic non-prefixed wrappers (see [`wrappers`]): `(dom-add-child
//!    dom child)` etc. dropping the `Az<Class>_` prefix.
//! 8. `(provide (all-defined-out))`.
//!
//! ## Callbacks are C-ABI direct (archetype A)
//!
//! Racket's `_fun` type constructor (built on `_cprocedure`) turns a
//! Racket closure into a **real C-callable function pointer** via a
//! libffi `ffi_closure`. So — unlike the libffi-*constrained* scripting
//! hosts (LuaJIT / CFFI / ruby-ffi can't return aggregates > 8 bytes
//! from a closure) — a Racket callback can be handed straight to the
//! C ABI. We still route callbacks through the shared *host-invoker*
//! plumbing (pointer-arg invokers + an out-pointer for the return) so
//! aggregate returns (`AzDom` from the layout callback, 240 bytes) work
//! uniformly and so the ctx/RefAny lifetime story matches every other
//! managed binding.
//!
//! ## GC-retention gotcha (handled in [`managed`])
//!
//! Racket is garbage-collected. The `ffi_closure` behind a `_fun`
//! callback is only kept alive while the Racket procedure it wraps is
//! reachable (`#:keep #t`, the default, ties the closure's lifetime to
//! the converted-procedure value). If the wrapping procedure is dropped,
//! the GC frees the closure and the next C call into it crashes. The
//! managed layer therefore pins every invoker closure in a module-level
//! `live-pins` list and stores every user callback in the module-level
//! `azul-handles` hash keyed by host-handle id — both are strong roots
//! for the process lifetime.

pub mod functions;
pub mod managed;
pub mod pkg;
pub mod types;
pub mod wrappers;

use anyhow::Result;

use super::config::CodegenConfig;
use super::generator::CodeBuilder;
use super::ir::CodegenIR;

/// Library base name used in `ffi-lib`. Racket's `ffi-lib` appends the
/// platform-specific extension (`.so` / `.dylib` / `.dll`) and tries the
/// `lib`-prefixed and bare forms automatically.
pub const LIB_NAME: &str = "azul";

/// Public entry point. Produces the complete `azul.rkt` source as a
/// `String`. The caller writes it to disk (e.g.
/// `target/codegen/azul.rkt`) and emits the accompanying `info.rkt`
/// via [`pkg::generate_info_rkt`].
pub fn generate(ir: &CodegenIR, config: &CodegenConfig) -> Result<String> {
    let mut builder = CodeBuilder::new("  ");

    emit_header(&mut builder);
    emit_library_load(&mut builder);

    // Type declarations (enums, structs, tagged unions, callback typedefs).
    types::generate_types(&mut builder, ir, config)?;

    // define-azul bindings (one per IR function).
    functions::generate_defines(&mut builder, ir, config)?;

    // Managed-FFI runtime helpers (host-invoker pattern + refany).
    managed::emit_managed(&mut builder, ir);

    // Idiomatic non-prefixed wrappers.
    wrappers::generate_wrappers(&mut builder, ir, config)?;

    // Everything defined at module top-level is public. `all-defined-out`
    // also surfaces the raw `Az*` FFI bindings + cstruct accessors
    // (make-AzDom, AzDom-…, set-AzDom-…!) for power users, mirroring the
    // `azul.C` escape hatch the LuaJIT binding exposes.
    builder.blank();
    builder.line(";; Export the whole surface: idiomatic wrappers, the managed-FFI");
    builder.line(";; helpers, and the raw Az* bindings + cstruct accessors for power users.");
    builder.line("(provide (all-defined-out))");

    Ok(builder.finish())
}

fn emit_header(builder: &mut CodeBuilder) {
    builder.line("#lang racket/base");
    builder.line(";;;; ============================================================================");
    builder.line(";;;; Auto-generated Racket (ffi/unsafe) bindings for the Azul GUI framework.");
    builder.line(";;;; Generated by azul-doc codegen v2 (lang_racket).");
    builder.line(";;;; DO NOT EDIT MANUALLY -- re-run the generator instead.");
    builder.line(";;;;");
    builder.line(";;;; Requires Racket 8.x (any recent CS or BC build). The whole surface");
    builder.line(";;;; is built on `ffi/unsafe`, which is libffi-backed, so Racket closures");
    builder.line(";;;; become REAL C function pointers (archetype A: C-ABI-direct callbacks).");
    builder.line(";;;; ============================================================================");
    builder.blank();
    builder.line("(require ffi/unsafe");
    builder.line("         ffi/unsafe/define)");
    builder.blank();
}

fn emit_library_load(builder: &mut CodeBuilder) {
    builder.line(";; ----------------------------------------------------------------------------");
    builder.line(";; Foreign library");
    builder.line(";;");
    builder.line(";; `ffi-lib` resolves libazul.so / libazul.dylib / azul.dll via the OS");
    builder.line(";; dynamic loader. Honour AZ_LIB_DIR (used by the e2e matrix, which copies");
    builder.line(";; the prebuilt library next to the example) by trying an absolute path");
    builder.line(";; first, then falling back to the bare name on the default search path.");
    builder.line(";; ----------------------------------------------------------------------------");
    builder.line("(define azul-lib");
    builder.line("  (let ([dir (getenv \"AZ_LIB_DIR\")])");
    builder.line("    (or (and dir (not (string=? dir \"\"))");
    builder.line(&format!(
        "             (ffi-lib (build-path dir \"{}\") #:fail (lambda () #f)))",
        LIB_NAME
    ));
    builder.line(&format!("        (ffi-lib \"{}\"))))", LIB_NAME));
    builder.blank();
    builder.line(";; The binding macro every Az* function is declared through. The default");
    builder.line(";; #:make-c-id keeps the Racket identifier verbatim as the C symbol name,");
    builder.line(";; so `(define-azul AzApp_create ...)` links the C symbol \"AzApp_create\".");
    builder.line(";; #:default-make-fail make-not-available defers a missing-symbol error to");
    builder.line(";; first use, so a partially-stripped release library still loads.");
    builder.line("(define-ffi-definer define-azul azul-lib");
    builder.line("  #:default-make-fail make-not-available)");
    builder.blank();
}

// =============================================================================
// Shared naming helpers (used by submodules)
// =============================================================================

/// The Racket ctype identifier for an IR type name. Racket's
/// `define-cstruct` / `_union` idiom keeps the C name verbatim with a
/// leading underscore, e.g. `AzDom` → `_AzDom`, matching the task's
/// `(_fun _AzRefAny _AzAppConfig -> _AzApp)` shape. A leading `Az` is
/// added when missing so the ctype unambiguously belongs to the bindings.
pub fn ctype_name(name: &str) -> String {
    let body = name.strip_prefix("Az").unwrap_or(name);
    format!("_Az{}", body)
}

/// The bare (no underscore) C identifier for constants / accessors, e.g.
/// `AzDom`. Used when composing `AzUpdate_RefreshDom`, `make-AzDom`, ...
pub fn c_name(name: &str) -> String {
    let body = name.strip_prefix("Az").unwrap_or(name);
    format!("Az{}", body)
}

/// Convert a snake_case / camelCase / PascalCase identifier (field, arg,
/// method name) to plain kebab-case. Racket idiom favours kebab-case.
pub fn kebab(name: &str) -> String {
    let mut out = String::new();
    let mut prev_lower = false;
    let mut prev_digit = false;
    for c in name.chars() {
        if c == '_' || c == '-' {
            if !out.is_empty() && !out.ends_with('-') {
                out.push('-');
            }
            prev_lower = false;
            prev_digit = false;
        } else if c.is_uppercase() {
            if (prev_lower || prev_digit) && !out.is_empty() && !out.ends_with('-') {
                out.push('-');
            }
            for lc in c.to_lowercase() {
                out.push(lc);
            }
            prev_lower = false;
            prev_digit = false;
        } else {
            out.push(c);
            prev_lower = c.is_ascii_lowercase();
            prev_digit = c.is_ascii_digit();
        }
    }
    while out.contains("--") {
        out = out.replace("--", "-");
    }
    sanitize_racket_ident(&out)
}

/// The idiomatic (kebab, no `az-` prefix) class name for a type.
pub fn idiomatic_class_name(name: &str) -> String {
    let body = name.strip_prefix("Az").unwrap_or(name);
    kebab(body)
}

/// Mangle identifiers that collide with a small set of `racket/base`
/// forms we don't want to shadow when used as procedure/argument names.
/// Racket has very few truly-reserved words; this is a defensive list.
pub fn sanitize_racket_ident(name: &str) -> String {
    const RESERVED: &[&str] = &[
        "define", "lambda", "let", "let*", "letrec", "if", "cond", "case", "when", "unless",
        "begin", "set!", "quote", "quasiquote", "require", "provide", "struct", "and", "or",
        "do", "for", "map", "list", "car", "cdr", "else", "λ",
    ];
    if RESERVED.contains(&name) {
        format!("{}*", name)
    } else {
        name.to_string()
    }
}

/// Map a Rust/IR type name to the corresponding Racket ctype expression.
///
/// Primitives lower to their `ffi/unsafe` ctype (`_uint32`, `_double`,
/// `_stdbool`, ...). Pointers/references collapse to `_pointer`. Struct
/// types surface as `_AzFoo`; unions as their `(_union ...)`-backed
/// `_AzFoo` alias; recursive types as `_pointer` (the C ABI boxes them).
pub fn map_type_to_racket(rust_type: &str, ir: &CodegenIR) -> String {
    let trimmed = rust_type.trim();

    // Pointers and references collapse to _pointer.
    if let Some(rest) = trimmed.strip_prefix("*const ") {
        if matches!(rest.trim(), "c_char" | "char" | "i8" | "u8") {
            // A const char* surfaces as an opaque pointer here (the string
            // wrappers copy bytes explicitly via AzString_copyFromBytes).
            return "_pointer".to_string();
        }
        return "_pointer".to_string();
    }
    if trimmed.starts_with("*mut ")
        || trimmed.starts_with("&mut ")
        || trimmed.starts_with('&')
    {
        return "_pointer".to_string();
    }

    // Arrays: `[T; N]` -> `(_array <elem> N)`.
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        let inner = &trimmed[1..trimmed.len() - 1];
        if let Some(semi) = inner.rfind(';') {
            let elem = inner[..semi].trim();
            if let Ok(count) = inner[semi + 1..].trim().parse::<usize>() {
                let elem_ct = map_type_to_racket(elem, ir);
                return format!("(_array {} {})", elem_ct, count);
            }
        }
    }

    match trimmed {
        "void" | "c_void" | "()" => "_void".to_string(),
        // The C ABI uses C99 `bool` (1 byte) — `_stdbool`, not `_bool`
        // (which is `int`-sized in Racket).
        "bool" | "GLboolean" => "_stdbool".to_string(),
        "i8" | "c_char" | "char" => "_int8".to_string(),
        "u8" | "c_uchar" => "_uint8".to_string(),
        "i16" => "_int16".to_string(),
        "u16" => "_uint16".to_string(),
        "i32" | "c_int" | "GLint" | "GLsizei" => "_int32".to_string(),
        "u32" | "c_uint" | "GLuint" | "GLenum" | "GLbitfield" => "_uint32".to_string(),
        "i64" | "GLint64" => "_int64".to_string(),
        "u64" | "GLuint64" => "_uint64".to_string(),
        "f32" | "GLfloat" | "GLclampf" => "_float".to_string(),
        "f64" | "GLdouble" | "GLclampd" => "_double".to_string(),
        "usize" | "size_t" | "uintptr_t" => "_uintptr".to_string(),
        "isize" | "ssize_t" | "intptr_t" | "GLsizeiptr" | "GLintptr" => "_intptr".to_string(),
        _ => {
            if let Some(s) = ir.find_struct(trimmed) {
                if matches!(s.category, super::ir::TypeCategory::Recursive) {
                    return "_pointer".to_string();
                }
                return ctype_name(trimmed);
            }
            if let Some(en) = ir.find_enum(trimmed) {
                if matches!(en.category, super::ir::TypeCategory::Recursive) {
                    return "_pointer".to_string();
                }
                // Both unit enums (int alias) and tagged unions ((_union …)
                // alias) surface under the same `_Az<Name>` ctype id.
                return ctype_name(trimmed);
            }
            if let Some(ta) = ir.find_type_alias(trimmed) {
                if ta.monomorphized_def.is_some() {
                    return ctype_name(trimmed);
                }
                // Simple alias: resolve transparently to the target's ctype.
                return map_type_to_racket(&ta.target, ir);
            }
            if ir.callback_typedefs.iter().any(|c| c.name == trimmed) {
                // Function-pointer typedefs: opaque C function pointer.
                return "_fpointer".to_string();
            }
            // Unknown -> opaque pointer.
            "_pointer".to_string()
        }
    }
}
