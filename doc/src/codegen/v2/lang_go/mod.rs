//! Go binding generator (cgo).
//!
//! Emits a small library of `.go` source files plus a `go.mod` manifest
//! that exposes `libazul`'s C-ABI to Go programs through cgo.
//!
//! # Why Go is C-tier (not S-tier)
//!
//! All other host-side bindings (C#, Ruby, Lua, Pascal, Ada, FreeBASIC,
//! Zig, PowerShell, PHP, Perl, OCaml) load `libazul` at runtime through
//! a dynamic-linker-style FFI (`dlopen` / `LoadLibrary` / P/Invoke /
//! `ffi.cdef` / `Interfaces.C` / etc.). The consumer needs **only** the
//! prebuilt shared library on their machine — no C toolchain, no header
//! file, no recompilation step.
//!
//! Go's cgo does **not** work that way. `import "C"` is a compile-time
//! directive that requires:
//!
//!   * a working C compiler (`gcc` / `clang` / MinGW) on the host,
//!   * `azul.h` on the C include path at build time,
//!   * `libazul.{so,dylib}` (or `azul.dll`) on the linker library path.
//!
//! cgo also makes cross-compilation genuinely painful: building a
//! Windows binary from Linux requires a MinGW cross-toolchain. We accept
//! these trade-offs because Go's audience justifies the inclusion, and
//! cgo at least delivers a fully native call path with no marshaller
//! overhead.
//!
//! # Strategy
//!
//! Like the Zig generator, we let the C compiler do the type translation.
//! The cgo prelude
//!
//! ```go
//! // #cgo LDFLAGS: -lazul
//! // #include "azul.h"
//! import "C"
//! ```
//!
//! makes every C type available as `C.AzApp`, `C.AzDom`, etc., and every
//! C function available as `C.AzApp_create(...)`. We don't redeclare the
//! FFI surface — we wrap it.
//!
//! We emit four Go source files plus `go.mod`:
//!
//! 1. `azul.go`  — package preamble, cgo `// #cgo` and `// #include`
//!                 directives, `import "C"`, and shared documentation.
//! 2. `types.go` — Go-side mirror types for the public surface (drops
//!                 the `Az` prefix), plus tagged-union sealed interfaces
//!                 and per-variant types. Skipped categories live here
//!                 as `// SKIPPED:` comments.
//! 3. `functions.go` — top-level constants (enum values) and helper
//!                 conversion functions. Most C functions get exposed as
//!                 methods on wrapper types in `wrappers.go` instead.
//! 4. `wrappers.go` — `type App struct { ptr *C.AzApp }` plus
//!                 constructors, instance methods, and `Close() error`
//!                 implementations of `io.Closer` for every type that
//!                 has an `_delete` C function. `runtime.SetFinalizer`
//!                 is registered as a safety net so leaks become eventual
//!                 cleanups instead of permanent ones.
//! 5. `go.mod`   — `module github.com/azul/azul-go` + Go 1.21 directive.
//!
//! # Output protocol
//!
//! `generate(ir, config)` returns a single concatenated `String` with
//! per-file sections separated by [`FILE_MARKER`]. The marker is a
//! syntactically valid Go line comment (`// ==FILE: <path> ==`), so the
//! combined text would still parse as one Go file in a pinch. The
//! orchestrator splits on the marker and writes each chunk to its
//! relative path under `target/codegen/v2/go/`.
//!
//! # User responsibilities (consumer-side)
//!
//! 1. Place `azul.h` somewhere `cgo` can find it (current dir works,
//!    or set `CGO_CFLAGS=-I/path/to/headers`).
//! 2. Place `libazul.{so,dylib}` (or `azul.dll`) on the linker path
//!    (current dir works with `-L.`, or set `CGO_LDFLAGS=-L/path/to/lib`).
//! 3. Ensure the same library is reachable at runtime
//!    (`LD_LIBRARY_PATH` on Linux, `DYLD_LIBRARY_PATH` on macOS,
//!    `PATH` on Windows). On Linux, `-Wl,-rpath,$ORIGIN` works too.

pub mod functions;
pub mod gomod;
pub mod types;
pub mod wrappers;

use anyhow::Result;

use super::config::CodegenConfig;
use super::generator::CodeBuilder;
use super::ir::CodegenIR;

/// File-marker header that introduces each per-file section in the
/// concatenated output. The orchestrator splits on lines that start
/// with this prefix.
pub const FILE_MARKER: &str = "// ==FILE: ";

/// Trailing marker that closes the file-marker header line.
pub const END_MARKER: &str = " ==";

/// Library name passed to the linker via `// #cgo LDFLAGS: -lazul`.
/// Must match the prebuilt artifact (`libazul.so` / `libazul.dylib` /
/// `azul.dll`).
pub const LIB_NAME: &str = "azul";

/// Public entry point. Generates the multi-file Go binding concatenated
/// into a single `String` with file markers between chunks.
pub fn generate(ir: &CodegenIR, config: &CodegenConfig) -> Result<String> {
    let azul = generate_azul_go(config)?;
    let types_src = types::generate(ir, config)?;
    let functions_src = functions::generate(ir, config)?;
    let wrappers_src = wrappers::generate(ir, config)?;
    let gomod_src = gomod::generate_go_mod();

    let mut out = String::with_capacity(
        azul.len() + types_src.len() + functions_src.len() + wrappers_src.len() + gomod_src.len() + 256,
    );
    push_section(&mut out, "azul.go", &azul);
    push_section(&mut out, "types.go", &types_src);
    push_section(&mut out, "functions.go", &functions_src);
    push_section(&mut out, "wrappers.go", &wrappers_src);
    push_section(&mut out, "go.mod", &gomod_src);
    Ok(out)
}

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
// azul.go (umbrella + cgo prelude)
// ============================================================================

/// Generate the `azul.go` umbrella file. This file owns the cgo prelude
/// (the `// #cgo` + `// #include` comment block immediately before
/// `import "C"`) and the package-level documentation. It contains no
/// Go declarations of its own — those live in the sibling files.
fn generate_azul_go(config: &CodegenConfig) -> Result<String> {
    let mut b = CodeBuilder::new(&config.indent);

    b.line("// ============================================================================");
    b.line("// azul.go - Go (cgo) bindings for the Azul GUI framework.");
    b.line("// Auto-generated by azul-doc codegen v2 (lang_go). DO NOT EDIT MANUALLY.");
    b.line("// ============================================================================");
    b.line("//");
    b.line("// Strategy: cgo's `import \"C\"` directive parses the existing `azul.h`");
    b.line("// header at compile time and exposes every `typedef`, `struct`, `enum`,");
    b.line("// `union`, function declaration, and macro it understands under the `C`");
    b.line("// namespace. We don't redeclare the FFI surface - we wrap it.");
    b.line("//");
    b.line("// Build-time requirements (these are NOT runtime-only loads like the");
    b.line("// other azul bindings):");
    b.line("//   * a working C compiler (gcc / clang / MinGW) on the host,");
    b.line("//   * `azul.h` reachable on the C include path,");
    b.line("//   * `libazul.{so,dylib}` (or `azul.dll`) on the linker library path.");
    b.line("//");
    b.line("// Runtime requirements:");
    b.line("//   * the same `libazul.{so,dylib}` (or `azul.dll`) reachable through");
    b.line("//     `LD_LIBRARY_PATH` (Linux), `DYLD_LIBRARY_PATH` (macOS), or `PATH`");
    b.line("//     (Windows). `-Wl,-rpath,$ORIGIN` also works on Linux when the");
    b.line("//     library sits next to the binary.");
    b.line("//");
    b.line("// Cross-compilation note: cgo makes cross-compiles genuinely painful.");
    b.line("// Building a Windows binary from Linux requires a MinGW cross-toolchain;");
    b.line("// building a Linux binary from macOS requires the corresponding sysroot.");
    b.line("// This is unavoidable for cgo-backed bindings.");
    b.line("// ============================================================================");
    b.blank();
    b.line("package azul");
    b.blank();

    // The cgo prelude MUST be a single comment block (no blank lines)
    // immediately followed by `import \"C\"`. Inserting any blank line or
    // top-level declaration between the comment and `import \"C\"` breaks
    // cgo. See https://pkg.go.dev/cmd/cgo for the rules.
    b.line("/*");
    b.line(&format!("#cgo LDFLAGS: -l{}", LIB_NAME));
    b.line("#include <stdlib.h>");
    b.line("#include <string.h>");
    b.line("#include \"azul.h\"");
    b.line("*/");
    b.line("import \"C\"");
    b.blank();
    b.line("import (");
    b.indent();
    b.line("\"unsafe\"");
    b.line("\"runtime\"");
    b.dedent();
    b.line(")");
    b.blank();

    // Reference unsafe/runtime so a build that happens to use only the
    // umbrella file still type-checks. The `_ = ...` blank assignments
    // are erased by the compiler.
    b.line("// Suppress unused-import errors when only the umbrella file is consumed.");
    b.line("var _ = unsafe.Sizeof(uintptr(0))");
    b.line("var _ = runtime.GC");
    b.blank();

    // String marshalling helpers used by every wrapper. These are public");
    // (capitalised) so user code can reach for them too.");
    b.line("// goString converts a C string allocated by libazul into a Go string.");
    b.line("// Does NOT free the underlying C string; callers must arrange for that");
    b.line("// separately if it was malloc'd.");
    b.line("func goString(p *C.char) string {");
    b.indent();
    b.line("if p == nil {");
    b.indent();
    b.line("return \"\"");
    b.dedent();
    b.line("}");
    b.line("return C.GoString(p)");
    b.dedent();
    b.line("}");
    b.blank();

    b.line("// cString allocates a C string from a Go string. The returned pointer");
    b.line("// must be freed with C.free; callers typically wrap the call site in");
    b.line("// `defer C.free(unsafe.Pointer(p))`.");
    b.line("func cString(s string) *C.char {");
    b.indent();
    b.line("return C.CString(s)");
    b.dedent();
    b.line("}");
    b.blank();

    b.line("// cBool converts a Go bool into the C-ABI's 1-byte boolean.");
    b.line("func cBool(b bool) C.bool {");
    b.indent();
    b.line("if b {");
    b.indent();
    b.line("return C.bool(true)");
    b.dedent();
    b.line("}");
    b.line("return C.bool(false)");
    b.dedent();
    b.line("}");
    b.blank();

    Ok(b.finish())
}

// ============================================================================
// Shared name-mangling and type-mapping helpers (used by submodules)
// ============================================================================

/// The `Az`-prefixed FFI type name for an IR type
/// (e.g. `Dom` -> `AzDom`).
pub fn ffi_type_name(name: &str) -> String {
    format!("Az{}", name)
}

/// The Go-side wrapper type name (no `Az` prefix), with reserved-word
/// mangling. Matches the convention `azul.NewApp`, `azul.Dom`, etc.
pub fn go_type_name(name: &str) -> String {
    sanitize_identifier(name)
}

/// Sanitize a name for use as a Go identifier. Go reserved words are
/// mangled with a trailing underscore (the convention used by the
/// stdlib's `cgo` tooling for the same purpose).
pub fn sanitize_identifier(name: &str) -> String {
    if is_go_keyword(name) {
        format!("{}_", name)
    } else {
        name.to_string()
    }
}

/// The complete set of Go reserved keywords, plus the predeclared
/// identifiers that, while technically re-bindable, would create
/// confusing user-facing wrappers.
fn is_go_keyword(s: &str) -> bool {
    matches!(
        s,
        "break"
            | "case"
            | "chan"
            | "const"
            | "continue"
            | "default"
            | "defer"
            | "else"
            | "fallthrough"
            | "for"
            | "func"
            | "go"
            | "goto"
            | "if"
            | "import"
            | "interface"
            | "map"
            | "package"
            | "range"
            | "return"
            | "select"
            | "struct"
            | "switch"
            | "type"
            | "var"
    )
}

/// Convert a snake_case method name to PascalCase for idiomatic Go
/// (Go uses PascalCase for exported identifiers).
pub fn snake_to_pascal(s: &str) -> String {
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

/// Convert an api.json method name (typically already lowerCamelCase or
/// snake_case) into an idiomatic exported Go method name (PascalCase).
///
/// `new` becomes `New` (used as a constructor prefix), `default` becomes
/// `Default`. Other names are PascalCased.
pub fn idiomatic_method_name(method_name: &str) -> String {
    let pascal = if method_name.contains('_') {
        snake_to_pascal(method_name)
    } else {
        let mut chars = method_name.chars();
        match chars.next() {
            None => String::new(),
            Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        }
    };
    // Every wrapper class gets a `Close()` method for io.Closer; a
    // user-API method named `close` would re-declare it (Go errors:
    // `method Close already declared`). Rename it. The SvgPath wrapper
    // hits this — `close` is the SVG path "close path" segment, not
    // a lifecycle operation.
    if pascal == "Close" {
        "CloseInner".to_string()
    } else {
        pascal
    }
}

/// PascalCase / camelCase -> snake_case. Mirrors the IR builder's
/// `to_snake_case` helper for class names (e.g. `StyleTextView` ->
/// `style_text_view`). Used to identify the implicit-self argument
/// the IR rewrites onto each instance method.
pub fn to_snake_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    let bytes = s.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        let c = b as char;
        if c.is_ascii_uppercase() {
            if i > 0 {
                let prev = bytes[i - 1] as char;
                let next = bytes.get(i + 1).map(|&n| n as char).unwrap_or(' ');
                let prev_lower_or_digit = prev.is_ascii_lowercase() || prev.is_ascii_digit();
                let next_lower = next.is_ascii_lowercase();
                if prev_lower_or_digit || (prev.is_ascii_uppercase() && next_lower) {
                    out.push('_');
                }
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// Map a Rust/IR primitive name to its Go equivalent. Returns `None`
/// for non-primitives (caller routes those through `C.<TypeName>`).
pub fn primitive_to_go(name: &str) -> Option<&'static str> {
    Some(match name {
        "bool" => "bool",
        "u8" | "c_uchar" => "uint8",
        "i8" | "c_char" | "char" => "int8",
        "u16" => "uint16",
        "i16" => "int16",
        "u32" | "c_uint" => "uint32",
        "i32" | "c_int" => "int32",
        "u64" => "uint64",
        "i64" => "int64",
        "f32" => "float32",
        "f64" => "float64",
        "usize" => "uint",
        "isize" => "int",
        "c_void" | "void" | "()" => "",
        _ => return None,
    })
}

/// Map a Rust/IR primitive name directly to its `C.*` cgo equivalent
/// (used in cgo call sites where we need to cast Go values into C
/// argument types). Returns `None` for non-primitives.
pub fn primitive_to_cgo(name: &str) -> Option<&'static str> {
    Some(match name {
        "bool" => "C.bool",
        "u8" | "c_uchar" => "C.uint8_t",
        "i8" | "c_char" | "char" => "C.int8_t",
        "u16" => "C.uint16_t",
        "i16" => "C.int16_t",
        "u32" | "c_uint" => "C.uint32_t",
        "i32" | "c_int" => "C.int32_t",
        "u64" => "C.uint64_t",
        "i64" => "C.int64_t",
        "f32" => "C.float",
        "f64" => "C.double",
        "usize" => "C.size_t",
        "isize" => "C.ssize_t",
        _ => return None,
    })
}
